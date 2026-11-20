' frmDrawing — the scrollbars set a shape's and a line's size. The readout
' follows the thumbs as they move, from a drag or from code (Change).

Sub Form_Load()
    ShowReadout
End Sub

Sub hsbWidth_Change()
    shpBox.Width = hsbWidth.Value
    lneRule.Width = hsbWidth.Value
    ShowReadout
End Sub

Sub vsbHeight_Change()
    shpBox.Height = vsbHeight.Value
    ShowReadout
End Sub

Sub ShowReadout()
    lblReadout.Caption = "Width " & hsbWidth.Value & "   Height " & vsbHeight.Value
End Sub

Sub cmdDone_Click()
    Unload Me
End Sub
