# 探针3：UIA 焦点元素 + OBJID_CARET 位置/状态位（查询全部在 C# 内完成）
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

Add-Type -ReferencedAssemblies Accessibility @'
using System;
using System.Runtime.InteropServices;
using Accessibility;
public class CaretProbe {
    [DllImport("oleacc.dll")]
    static extern int AccessibleObjectFromWindow(IntPtr hwnd, uint id, ref Guid iid, [MarshalAs(UnmanagedType.IUnknown)] out object ppv);
    [DllImport("user32.dll")]
    static extern IntPtr GetForegroundWindow();
    public static string Query() {
        Guid iid = new Guid("618736e0-3c3d-11cf-810c-00aa00389b71");
        object o;
        int hr = AccessibleObjectFromWindow(GetForegroundWindow(), 0xFFFFFFF8u, ref iid, out o);
        if (hr != 0 || o == null) return "hr=" + hr;
        IAccessible acc = (IAccessible)o;
        int x = 0, y = 0, w = 0, h = 0;
        acc.accLocation(out x, out y, out w, out h, 0);
        object st = acc.get_accState(0);
        uint s;
        try { s = Convert.ToUInt32(st); } catch { s = 0xFFFFFFFFu; }
        return string.Format("loc=({0},{1},{2},{3}) state=0x{4:X}", x, y, w, h, s);
    }
}
'@

$last = ""
while ($true) {
    $focusDesc = "?"
    try {
        $e = [System.Windows.Automation.AutomationElement]::FocusedElement
        $pats = ($e.GetSupportedPatterns() | ForEach-Object { $_.ProgrammaticName -replace 'Identifiers\.Pattern$','' }) -join ','
        $focusDesc = "$($e.Current.ControlType.ProgrammaticName) cls=$($e.Current.ClassName) pats=$pats"
    } catch { $focusDesc = "UIA-ERR" }
    $caretDesc = try { [CaretProbe]::Query() } catch { "ERR $($_.Exception.Message)" }

    $line = "FOCUS: $focusDesc  ||  CARET: $caretDesc"
    if ($line -ne $last) { Write-Host ("[{0}] {1}" -f (Get-Date -Format HH:mm:ss.fff), $line); $last = $line }
    Start-Sleep -Milliseconds 200
}
