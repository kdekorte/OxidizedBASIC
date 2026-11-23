' files.bas — sequential file I/O: write records, read them back, append, EOF,
' and a missing file. Run it from this folder:
'
'     oxbasic run examples/files/files.bas
'
' It writes scores.txt next to this program and prints what it reads back.

Sub WriteScores()
    Open "scores.txt" For Output As #1
    ' Write quotes text and separates the values with commas: one record a line
    Write #1, "Ada", 91, 88.5
    Write #1, "Grace, H", 77, 94
    Close #1
    ' a plain text file, read back a line at a time with Line Input
    Open "notes.txt" For Output As #5
    Print #5, "Scores are out of 100."
    Print #5, "Alan asked for a re-mark."
    Close #5
End Sub

Sub AppendScore()
    Open "scores.txt" For Append As #1
    Write #1, "Linus", 85, 79
    Close #1
End Sub

Sub ReadScores()
    Dim name As String, a As Long, b As Double, total As Double, count As Long
    Open "scores.txt" For Input As #2
    Do While Not EOF(2)
        Input #2, name, a, b
        Print name; " scored "; a; " and "; b
        total = total + a
        count = count + 1
    Loop
    Close #2
    Print "Records: "; count
    Print "Average of the first scores: "; total / count
End Sub

Sub ReadLines()
    Dim line As String, n As Long
    Open "notes.txt" For Input As #3
    Do While Not EOF(3)
        Line Input #3, line
        n = n + 1
        Print n; ": "; line
    Loop
    Close #3
End Sub

Sub TryMissing()
    ' a file that is not there raises error 53; the handler reports it and
    ' leaves the sub, so nothing that needs the file runs
    On Error GoTo Missing
    Open "no_such_file.txt" For Input As #4
    Print "opened"
    Close #4
    Exit Sub
Missing:
    Print "Error "; Err.Number; ": "; Err.Description
    Exit Sub
End Sub

WriteScores
AppendScore
ReadScores
ReadLines
TryMissing
Print "Done"
