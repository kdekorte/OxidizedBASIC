' records.bas: random and binary file I/O with user Types, in VB's binary layout.
' Run it from this folder:
'
'     oxbasic run examples/records/records.bas
'
' It writes people.dat (fixed-length records) and raw.bin (bytes), then reads
' them back. The files are rewritten each run.

Type Address
    Street As String * 12
    Zip As Long
End Type

Type Person
    Name As String * 8
    Age As Integer
    Home As Address
End Type

Sub WritePeople()
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
End Sub

Sub ReadPeople()
    Dim p As Person
    Dim i As Integer
    Open "people.dat" For Random As #2 Len = 32
    Print "Records: "; LOF(2) / 32
    For i = 3 To 1 Step -1
        Get #2, i, p
        Print i; ": "; RTrim(p.Name); " age "; p.Age; " at "; RTrim(p.Home.Street); " "; p.Home.Zip
    Next i
    Close #2
End Sub

Sub WriteRaw()
    Dim n As Integer
    Dim text As String * 5
    Open "raw.bin" For Binary As #3
    text = "HELLO"
    Put #3, , text
    n = 258
    Put #3, , n
    Close #3
End Sub

Sub ReadRaw()
    Dim s As String * 5
    Dim n As Integer
    Open "raw.bin" For Binary As #4
    Get #4, , s
    Print "Text: "; RTrim(s)
    Seek #4, 6
    Get #4, , n
    Print "Number: "; n
    Print "Length: "; LOF(4)
    Close #4
End Sub

WritePeople
ReadPeople
WriteRaw
ReadRaw
Print "Done"
