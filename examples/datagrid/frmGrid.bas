' frmGrid: a DataGrid to try by hand.
'   click a cell, or move with the arrow keys / Tab: the status line follows
'   type into a cell (Enter keeps it, Esc cancels); "bad" is refused
'   the wheel or the scrollbars move the view: the top row shows in the status
'   the buttons change the grid from the program side

Sub Form_Load()
    Me.Caption = "DataGrid example"
    lblStatus.Caption = "Click a cell, or type into one. Try typing ""bad""."
    lblEdit.Caption = "Last edit: none"
End Sub

Sub Grid1_RowColChange()
    lblStatus.Caption = "Cell row " & Grid1.Row & ", column " & Grid1.Col & "   (value: " & Grid1.Text & ")"
End Sub

Sub Grid1_Scroll()
    lblStatus.Caption = "Scrolled: top row is " & Grid1.TopRow
End Sub

' ValidateEdit runs before a typed value is stored; Cancel = 1 refuses it.
' Grid1.Text is the text being typed while this runs.
' Change runs on every key while a cell is being typed in: Grid1.Text is the text so far.
Sub Grid1_Change()
    lblStatus.Caption = "Typing: " & Grid1.Text
End Sub

Sub Grid1_ValidateEdit(Row, Col, Cancel)
    If Grid1.Text = "bad" Then
        Cancel = 1
        lblStatus.Caption = "Refused: ""bad"" is not allowed in row " & Row
    End If
End Sub

Sub Grid1_AfterEdit(Row, Col)
    lblEdit.Caption = "Last edit: row " & Row & ", column " & Col & " = " & Grid1.TextMatrix(Row, Col)
End Sub

Sub cmdAddRow_Click()
    Dim n
    n = Grid1.Rows
    Grid1.Rows = n + 1
    Grid1.TextMatrix(n, 0) = Str(n)
    Grid1.TextMatrix(n, 1) = "New item"
    lblStatus.Caption = "Added row " & n & "."
End Sub

Sub cmdFill_Click()
    Dim r, q
    For r = 1 To Grid1.Rows - 1
        q = Grid1.TextMatrix(r, 2)
        If q <> "" Then
            Grid1.TextMatrix(r, 2) = Str(Val(q) * 10)
        End If
    Next
    lblStatus.Caption = "Multiplied each Qty by 10."
End Sub

Sub cmdClear_Click()
    Dim r, c
    For r = 1 To Grid1.Rows - 1
        For c = 1 To Grid1.Cols - 1
            Grid1.TextMatrix(r, c) = ""
        Next
    Next
    lblEdit.Caption = "Last edit: none"
    lblStatus.Caption = "Cleared the body."
End Sub

Sub cmdDone_Click()
    Unload Me
End Sub
