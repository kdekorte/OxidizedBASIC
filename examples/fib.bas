' fib.bas — recursion, ByRef, and Select Case

Function Fib(n As Long) As Long
    Select Case n
        Case 0, 1
            Fib = n
        Case Is > 1
            Fib = Fib(n - 1) + Fib(n - 2)
        Case Else
            Err.Raise 5, "negative input"
    End Select
End Function

Sub Accumulate(ByRef total As Long, ByVal n As Long)
    total = total + Fib(n)
End Sub

Dim total As Long
Dim i As Long
For i = 1 To 10
    Accumulate total, i
Next
Print "Fib(1..10) sum ="; total
