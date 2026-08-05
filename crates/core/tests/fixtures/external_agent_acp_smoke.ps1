$null = [Console]::ReadLine()
[Console]::WriteLine('{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1}}')

$null = [Console]::ReadLine()
[Console]::WriteLine('{"jsonrpc":"2.0","id":2,"result":{"sessionId":"smoke-session"}}')

$null = [Console]::ReadLine()
[Console]::WriteLine('{"jsonrpc":"2.0","id":10,"method":"fs/read_text_file","params":{"path":"input.txt"}}')
$read = [Console]::ReadLine() | ConvertFrom-Json
if ($read.result.content -ne 'source') { exit 10 }

[Console]::WriteLine('{"jsonrpc":"2.0","id":11,"method":"fs/write_text_file","params":{"path":"output.txt","content":"created"}}')
$write = [Console]::ReadLine() | ConvertFrom-Json
if ($write.id -ne 11) { exit 11 }

[Console]::WriteLine('{"jsonrpc":"2.0","id":12,"method":"terminal/create","params":{"command":"powershell.exe","args":["-NoProfile","-Command","Write-Output terminal-ok"],"cwd":".","outputByteLimit":4096}}')
$terminal = [Console]::ReadLine() | ConvertFrom-Json
$terminalId = $terminal.result.terminalId
if ([string]::IsNullOrWhiteSpace($terminalId)) { exit 12 }

[Console]::WriteLine((@{jsonrpc='2.0'; id=13; method='terminal/wait_for_exit'; params=@{terminalId=$terminalId}} | ConvertTo-Json -Compress))
$wait = [Console]::ReadLine() | ConvertFrom-Json
if ($wait.id -ne 13) { exit 13 }

[Console]::WriteLine((@{jsonrpc='2.0'; id=14; method='terminal/output'; params=@{terminalId=$terminalId}} | ConvertTo-Json -Compress))
$output = [Console]::ReadLine() | ConvertFrom-Json
if (-not $output.result.output.Contains('terminal-ok')) { exit 14 }

[Console]::WriteLine('{"jsonrpc":"2.0","id":4,"method":"session/request_permission","params":{"options":[{"optionId":"reject-once","kind":"reject_once"}]}}')
$permission = [Console]::ReadLine() | ConvertFrom-Json
if ($permission.result.outcome.outcome -ne 'cancelled') { exit 15 }

[Console]::WriteLine('{"jsonrpc":"2.0","id":5,"method":"session/request_permission","params":{"options":[{"optionId":"allow-once","kind":"allow_once"}]}}')
$permission = [Console]::ReadLine() | ConvertFrom-Json
if ($permission.result.outcome.optionId -ne 'allow-once') { exit 16 }

[Console]::WriteLine('{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"smoke-session","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"acp ok"}}}}')
[Console]::WriteLine('{"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn"}}')
