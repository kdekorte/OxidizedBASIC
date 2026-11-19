' Module1 — a standard module: its Subs and Functions are shared by every
' form of the project (the forms' own handlers and variables are not).

Function Plural(n As Long, word As String) As String
    If n = 1 Then
        Plural = n & " " & word
    Else
        Plural = n & " " & word & "s"
    End If
End Function

Function Describe(n As Long) As String
    Select Case n
        Case 0
            Describe = "none"
        Case 1
            Describe = "one"
        Case Else
            Describe = "several"
    End Select
End Function
