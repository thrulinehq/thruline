"""Bounded OpenHands native options. Transport never selects an external host."""
def validate(value):
    if value is None:
        return {'schema':'thruline.openhands-options.v1','backend':'local','condenser':None}
    if not isinstance(value,dict) or set(value)-{'schema','backend','condenser'}:
        raise ValueError('unsupported OpenHands options')
    if value.get('schema') != 'thruline.openhands-options.v1' or value.get('backend') not in ['local','owned-server']:
        raise ValueError('invalid OpenHands native backend/schema')
    condenser=value.get('condenser')
    if condenser is not None:
        if not isinstance(condenser,dict) or set(condenser)!={'max_size','keep_first','max_tokens'}:
            raise ValueError('condenser requires exact bounded native fields')
        maximum, first, tokens = condenser['max_size'],condenser['keep_first'],condenser['max_tokens']
        if type(maximum) is not int or not 8<=maximum<=240:
            raise ValueError('invalid condenser event bound')
        if type(first) is not int or not 0<=first<maximum//2-1:
            raise ValueError('condenser must leave a valid native forgetting range')
        if tokens is not None and (type(tokens) is not int or not 2048<=tokens<=16384):
            raise ValueError('invalid condenser token bound')
    return {'schema':value['schema'],'backend':value['backend'],'condenser':condenser}
