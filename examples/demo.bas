' demo.bas — a tour of the Phase 1 language surface

Option Explicit

Dim Names() As String

Function FizzBuzz(n As Long) As String
    If n Mod 15 = 0 Then
        FizzBuzz = "FizzBuzz"
    ElseIf n Mod 3 = 0 Then
        FizzBuzz = "Fizz"
    ElseIf n Mod 5 = 0 Then
        FizzBuzz = "Buzz"
    Else
        FizzBuzz = CStr(n)
    End If
End Function

Sub ShowHeader(title As String)
    Print "== " & UCase(title) & " =="
End Sub

' --- main ---
ShowHeader "oxidizedbasic demo"

Dim i As Long
For i = 1 To 15
    Print FizzBuzz(i);
Next
Print ""

ShowHeader "arrays"
Dim squares(9) As Long
Dim v As Variant
For i = 0 To 9
    squares(i) = i * i
Next
For Each v In squares
    Print v;
Next
Print ""

ShowHeader "strings"
Dim s As String
s = "  OxidizedBASIC  "
Print "trimmed: " & Trim(s)
Print "words: " & CStr(UBound(Split(Trim(s), " ")) + 1)
Print "reversed: " & StrReverse(Trim(s))

ShowHeader "dates"
Dim d As Date
d = DateSerial(2026, 11, 17)
Print Format(d, "yyyy-mm-dd") & " + 30d = " & Format(DateAdd("d", 30, d), "yyyy-mm-dd")

ShowHeader "error handling"
On Error GoTo Oops
Dim x As Long
x = 1 / 0 ' raises runtime error 11
Print "unreachable"
End

Oops:
    Print "caught error " & CStr(Err.Number) & ": " & Err.Description
    Resume Next
