' Module1 — file I/O routines shared by the form. Each demo writes its files
' next to the project, and returns what it found as text, so the form can show
' it in its text box. A standard module cannot reach a form's controls itself.

Function SequentialDemo() As String
    Dim out As String
    Dim name As String, a As Long, b As Double, total As Double, count As Long
    Dim line As String, n As Long

    ' write two sequential files: comma-separated records and plain text lines
    Open "scores.txt" For Output As #1
    Write #1, "Ada", 91, 88.5
    Write #1, "Grace, H", 77, 94
    Close #1
    Open "scores.txt" For Append As #1
    Write #1, "Linus", 85, 79
    Close #1
    Open "notes.txt" For Output As #5
    Print #5, "Scores are out of 100."
    Print #5, "Alan asked for a re-mark."
    Close #5

    ' read the records back with Input #
    Open "scores.txt" For Input As #2
    Do While Not EOF(2)
        Input #2, name, a, b
        out = out & name & " scored " & a & " and " & b & vbCrLf
        total = total + a
        count = count + 1
    Loop
    Close #2
    out = out & "Records: " & count & vbCrLf
    out = out & "Average of the first scores: " & total / count & vbCrLf

    ' read a text file a line at a time with Line Input #
    Open "notes.txt" For Input As #3
    Do While Not EOF(3)
        Line Input #3, line
        n = n + 1
        out = out & n & ": " & line & vbCrLf
    Loop
    Close #3

    ' a missing file raises error 53, which the caller can trap
    On Error GoTo Missing
    Open "no_such_file.txt" For Input As #4
    out = out & "opened" & vbCrLf
    Close #4
    SequentialDemo = out
    Exit Function
Missing:
    out = out & "Error " & Err.Number & ": " & Err.Description & vbCrLf
    SequentialDemo = out
End Function

Type Address
    Street As String * 12
    Zip As Long
End Type

Type Person
    Name As String * 8
    Age As Integer
    Home As Address
End Type

Function RandomDemo() As String
    Dim out As String
    Dim p As Person
    Dim i As Integer

    Open "people.dat" For Random As #1 Len = 32
    For i = 1 To 3
        p.Name = "Person" & i
        p.Age = 20 + i
        p.Home.Street = "Main St " & i
        p.Home.Zip = 10000 + i
        Put #1, i, p
    Next i
    Close #1

    Open "people.dat" For Random As #2 Len = 32
    out = out & "Records: " & LOF(2) / 32 & vbCrLf
    For i = 3 To 1 Step -1
        Get #2, i, p
        out = out & i & ": " & RTrim(p.Name) & " age " & p.Age & " at " & RTrim(p.Home.Street) & " " & p.Home.Zip & vbCrLf
    Next i
    Close #2
    RandomDemo = out
End Function

Function BinaryDemo() As String
    Dim out As String
    Dim text As String * 5
    Dim n As Integer

    Open "raw.bin" For Binary As #3
    text = "HELLO"
    Put #3, , text
    n = 258
    Put #3, , n
    Close #3

    Open "raw.bin" For Binary As #4
    Get #4, , text
    out = out & "Text: " & RTrim(text) & vbCrLf
    Seek #4, 6
    Get #4, , n
    out = out & "Number: " & n & vbCrLf
    out = out & "Length: " & LOF(4) & vbCrLf
    Close #4
    BinaryDemo = out
End Function
