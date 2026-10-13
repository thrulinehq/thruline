"""Native pinned SDK conversations and owned loopback Agent Server transport.

The caller owns the outer Linux namespace and model broker. A server here is a
child process in that namespace, never an arbitrary remote URL or Docker socket.
"""
import contextlib
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import traceback
import uuid

from openhands_options import validate


def write(path, value):
    path.write_text(json.dumps(value, default=str, indent=2)+'\n')


def append(path, value):
    with path.open('a') as stream:
        stream.write(json.dumps(value, default=str)+'\n')


@contextlib.contextmanager
def server(output):
    import httpx
    if os.environ.get('THRULINE_NATIVE_NAMESPACE')!='linux':
        raise ValueError('owned native server requires Linux namespace')
    config = {'conversations_path':str(output/'server-persistence'),
              'workspace_path':'/workspace','bash_events_dir':str(output/'server-bash'),
              'enable_vscode':False,'preload_tools':False,'max_concurrent_runs':1,
              'telemetry':{'exporter':'none'}}
    write(output/'server-config.json',config)
    token=uuid.uuid4().hex  # Ephemeral guest-only authentication, no external account.
    env={**os.environ,'SESSION_API_KEY':token,'OPENHANDS_AGENT_SERVER_CONFIG_PATH':str(output/'server-config.json'),
         'LOG_JSON':'true','OH_TELEMETRY_CONSENT':'false','OH_ENABLE_VSCODE':'false',
         'OH_PRELOAD_TOOLS':'false','OPENHANDS_SETTINGS_DIR':str(output/'server-settings')}
    argv=[sys.executable,'/drivers/openhands_server.py','--host','127.0.0.1','--port','18081']
    lifecycle={'argv':argv,'endpoint':'http://127.0.0.1:18081','started_at':time.time(),
               'authentication':'ephemeral guest-only X-Session-API-Key','config':config}
    with (output/'server.stdout.log').open('wb') as stdout,(output/'server.stderr.log').open('wb') as stderr:
        process=subprocess.Popen(argv,stdout=stdout,stderr=stderr,env=env,cwd='/native')
        lifecycle['pid']=process.pid
        try:
            deadline=time.monotonic()+40
            health=[]
            while time.monotonic()<deadline:
                try:
                    response=httpx.get('http://127.0.0.1:18081/health',timeout=1,trust_env=False)
                    health.append({'status':response.status_code,'body':response.text})
                    if response.status_code==200:
                        break
                except httpx.RequestError as error:
                    health.append({'error':str(error)})
                if process.poll() is not None:
                    raise RuntimeError('native server exited before readiness')
                time.sleep(.25)
            else:
                raise TimeoutError('native server readiness deadline exceeded')
            lifecycle['health']=health
            # Actual native auth boundary; never infer this from configuration.
            denied=httpx.get('http://127.0.0.1:18081/api/conversations/search',headers={'X-Session-API-Key':'invalid-controlled-key'},timeout=5,trust_env=False)
            lifecycle['unauthorized_probe']={'status':denied.status_code,'body':denied.text}
            if denied.status_code not in [401,403]:
                raise RuntimeError('native server accepted an unauthorized request')
            yield 'http://127.0.0.1:18081',token
        finally:
            if process.poll() is None:
                process.terminate()
                try:process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill();process.wait(timeout=5)
            lifecycle['exit_code']=process.returncode
            lifecycle['stopped_at']=time.time()
            try:
                response=httpx.get('http://127.0.0.1:18081/health',timeout=1,trust_env=False)
                lifecycle['after_shutdown']={'status':response.status_code,'body':response.text}
            except httpx.RequestError as error:
                lifecycle['after_shutdown']={'error_type':type(error).__name__,'error':str(error)}
            write(output/'server-lifecycle.json',lifecycle)


def run(a, task):
    import litellm
    from openhands.sdk import LLM,Agent,Tool
    from openhands.sdk.context.condenser import LLMSummarizingCondenser
    from openhands.sdk.context.view import View
    from openhands.sdk.conversation.impl.local_conversation import LocalConversation
    from openhands.sdk.conversation.impl.remote_conversation import RemoteConversation
    from openhands.sdk.workspace.remote.base import RemoteWorkspace
    from openhands.tools.terminal import TerminalTool
    from openhands.tools.file_editor import FileEditorTool
    options=validate(json.loads(a.openhands_json) if a.openhands_json else None)
    if options['backend']=='owned-server' and (a.resume_id or a.interrupt_after_observations):
        raise ValueError('remote server persistence handoff is not yet exposed by this route')
    write(a.output/'openhands-options.json',options)
    class NativeProviderLog(litellm.integrations.custom_logger.CustomLogger):
        def log_pre_api_call(self,model,messages,kwargs):
            append(a.output/'provider-inputs.jsonl',{'model':model,'messages':messages,'optional_params':kwargs.get('optional_params'),'additional_args':kwargs.get('additional_args')})
        def log_post_api_call(self,kwargs,response_obj,start_time,end_time):
            append(a.output/'provider-responses.jsonl',{'response':response_obj})
    litellm.callbacks=[NativeProviderLog()]
    litellm.OllamaChatConfig(num_ctx=16384,seed=0)
    llm=LLM(usage_id='agent',model='ollama_chat/'+a.model_name,api_key='local-no-secret',base_url='http://127.0.0.1:11435',
            native_tool_calling=True,capability_overrides={'supports_sampling_params':True},temperature=0,reasoning_effort='none',
            max_input_tokens=16384,max_output_tokens=1600,num_retries=0,timeout=150,log_completions=True,
            log_completions_folder=str(a.output/'completions'))
    condenser=LLMSummarizingCondenser(llm=llm.model_copy(update={'usage_id':'condenser'}),**options['condenser']) if options['condenser'] else None
    agent=Agent(llm=llm,tools=[Tool(name=TerminalTool.name,params={'terminal_type':'subprocess','shell_path':'/bin/bash'}),Tool(name=FileEditorTool.name)],**({'condenser':condenser} if condenser else {}))
    cid=uuid.UUID(a.resume_id or a.conversation_id) if a.resume_id or a.conversation_id else uuid.uuid4()
    (a.output/'conversation-id.txt').write_text(str(cid))
    conversation=None;count=0;interrupted=False
    def callback(event):
        nonlocal count,interrupted
        append(a.output/'events.jsonl',event.model_dump(mode='json'))
        if event.kind=='ObservationEvent':
            count+=1
            if a.interrupt_after_observations and count>=a.interrupt_after_observations and not interrupted:
                interrupted=True
                write(a.output/'interruption-request.json',{'native_api':'LocalConversation.interrupt()','observations':count,'trigger_event':event.model_dump(mode='json'),'conversation_id':str(cid)})
                conversation.interrupt()
    effective=task+'\nTerminal working directory: /workspace. Selected editable source: '+json.dumps(a.files)+'. Only those source files may change. Start by inspecting actual source. Every intermediate response must call a native tool; finish via finish tool after checking the repair. Git and context files are read-only. Do not create test files.'
    (a.output/'effective-task.txt').write_text(effective)
    context=server(a.output) if options['backend']=='owned-server' else contextlib.nullcontext(None)
    result={'execution_status':'unstarted','action_count':0,'observation_count':0}
    try:
        with context as endpoint:
            workspace=None
            try:
                if endpoint:
                    host,key=endpoint
                    workspace=RemoteWorkspace(host=host,api_key=key,working_dir='/workspace',read_timeout=180)
                    def response_hook(response):
                        response.read();request=response.request
                        append(a.output/'server-transport.jsonl',{'method':request.method,'url':str(request.url),'request_body':request.content.decode('utf8'),'response_status':response.status_code,'response_body':response.text,
                           'request_sha256':hashlib.sha256(request.content).hexdigest(),'response_sha256':hashlib.sha256(response.content).hexdigest()})
                    workspace.client.event_hooks['response'].append(response_hook)
                    write(a.output/'server-info.json',workspace.get_server_info())
                    conversation=RemoteConversation(agent=agent,workspace=workspace,conversation_id=cid,callbacks=[callback],max_iteration_per_run=a.iterations,visualizer=None)
                else:
                    conversation=LocalConversation(agent=agent,workspace=str(a.workspace),conversation_id=cid,persistence_dir=str(a.output/'persistence'),callbacks=[callback],max_iteration_per_run=a.iterations,profile_store_dir=str(a.output/'profiles'),visualizer=None)
                conversation.send_message(effective)
                conversation.run()
            finally:
                if conversation:
                    try:
                        events=list(conversation.state.events)
                        write(a.output/'native-state.json',conversation.state.model_dump(mode='json'))
                        write(a.output/'native-metrics.json',conversation.conversation_stats.model_dump(mode='json'))
                        write(a.output/'native-context-view.json',View.from_events(events).model_dump(mode='json'))
                        write(a.output/'native-all-events.json',[e.model_dump(mode='json') for e in events])
                        current_events=[json.loads(line) for line in (a.output/'events.jsonl').read_text().splitlines()] if (a.output/'events.jsonl').exists() else []
                        result={'execution_status':conversation.state.execution_status.value,'action_count':sum(e.get('kind')=='ActionEvent' for e in current_events),'observation_count':sum(e.get('kind')=='ObservationEvent' for e in current_events),
                                'condensation_count':sum(e.get('kind')=='Condensation' for e in current_events),'persisted_event_count':len(events),'backend':options['backend']}
                    finally:conversation.close()
                if workspace:workspace.reset_client()
    except Exception as error:
        write(a.output/'native-failure.json',{'type':type(error).__name__,'message':str(error),'traceback':traceback.format_exc()})
        result['driver_error']=str(error)
    write(a.output/'native-result.json',result)
    return 0 if result['execution_status']=='finished' and result['action_count'] and not result.get('driver_error') else 3
