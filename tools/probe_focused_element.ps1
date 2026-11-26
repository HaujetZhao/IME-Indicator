# 探针：每 300ms 打印当前 UIA 焦点元素的身份信息，仅变化时输出
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

$last = ""
while ($true) {
    try {
        $e = [System.Windows.Automation.AutomationElement]::FocusedElement
        $type = $e.Current.ControlType.ProgrammaticName
        $cls  = $e.Current.ClassName
        $name = $e.Current.Name
        if ($name.Length -gt 30) { $name = $name.Substring(0, 30) }
        $pats = ($e.GetSupportedPatterns() | ForEach-Object { $_.ProgrammaticName }) -join ','
        $line = "$type | cls=$cls | name=$name | pats=$pats"
        if ($line -ne $last) {
            Write-Host ("[{0}] {1}" -f (Get-Date -Format HH:mm:ss), $line)
            $last = $line
        }
    } catch {
        $err = "ERR: $($_.Exception.Message)"
        if ($err -ne $last) { Write-Host $err; $last = $err }
    }
    Start-Sleep -Milliseconds 300
}
