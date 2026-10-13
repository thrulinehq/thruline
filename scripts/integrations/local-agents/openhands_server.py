"""Owned guest server entrypoint with the same pinned local model sampling."""
from pathlib import Path
import litellm
from openhands_native import append

class ProviderLog(litellm.integrations.custom_logger.CustomLogger):
    def log_pre_api_call(self,model,messages,kwargs):
        append(Path('/native/provider-inputs.jsonl'),{'model':model,'messages':messages,'optional_params':kwargs.get('optional_params'),'additional_args':kwargs.get('additional_args')})
    def log_post_api_call(self,kwargs,response_obj,start_time,end_time):
        append(Path('/native/provider-responses.jsonl'),{'response':response_obj})

litellm.callbacks=[ProviderLog()]
litellm.OllamaChatConfig(num_ctx=16384,seed=0)
from openhands.agent_server.__main__ import main
main()
