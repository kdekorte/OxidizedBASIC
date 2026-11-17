' clickme.bas — Phase 2 demo: GPUI form events driving BASIC code
' Run with:  oxbasic gui examples/clickme.oxform examples/clickme.bas

Dim Clicks As Long
Dim Ticks As Long

Sub Form_Load()
    MsgBox "Form loaded — Phase 2 GPUI binding engine online"
End Sub

Sub Command1_Click()
    Clicks = Clicks + 1
    MsgBox "Clicks: " & CStr(Clicks)
End Sub

Sub Command2_Click()
    Clicks = 0
    MsgBox "Clicks: " & CStr(Clicks)
End Sub

Sub Text1_Change()
    MsgBox "typed!"
End Sub

Sub List1_Click()
    MsgBox "fruit selected"
End Sub

Sub Timer1_Timer()
    Ticks = Ticks + 1
    If Ticks Mod 10 = 0 Then
        MsgBox "alive for " & CStr(Ticks) & " seconds"
    End If
End Sub
