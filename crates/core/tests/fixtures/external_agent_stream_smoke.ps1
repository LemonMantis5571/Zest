param(
    [string]$Task
)

[Console]::WriteLine('{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"hello"}}}')
[Console]::WriteLine('{"type":"stream_event","event":{"type":"content_block_start","content_block":{"type":"tool_use","id":"tool-1","name":"Read"}}}')
[Console]::WriteLine('{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"tool-1"}]}}')
[Console]::WriteLine('{"type":"result","response":"hello"}')
