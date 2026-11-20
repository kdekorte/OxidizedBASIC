' frmDrawing — the scrollbars set a shape's and a line's size. The readout
' follows the thumbs as they move, from a drag or from code (Change).

Sub Form_Load()
    ShowReadout
End Sub

Sub hsbWidth_Change()
    shpBox.Width = hsbWidth.Value
    lneRule.Width = hsbWidth.Value
    sldWidth.Value = hsbWidth.Value
    prgWidth.Value = hsbWidth.Value
    ShowReadout
End Sub

' The Slider is a second way to set the width: it moves the width bar too.
Sub sldWidth_Change()
    hsbWidth.Value = sldWidth.Value
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
