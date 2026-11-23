' frmRecords — runs the file I/O demos from Module1 and shows their output.
' The demos write their files next to this project.

Sub Form_Load()
    Me.Caption = "File I/O Examples"
    lblStatus.Caption = "Pick a demo, or Run all."
End Sub

Sub cmdSequential_Click()
    txtOutput.Text = SequentialDemo()
    lblStatus.Caption = "Sequential files: done."
End Sub

Sub cmdRandom_Click()
    txtOutput.Text = RandomDemo()
    lblStatus.Caption = "Random records: done."
End Sub

Sub cmdBinary_Click()
    txtOutput.Text = BinaryDemo()
    lblStatus.Caption = "Binary bytes: done."
End Sub

Sub cmdAll_Click()
    txtOutput.Text = SequentialDemo() & vbCrLf & RandomDemo() & vbCrLf & BinaryDemo()
    lblStatus.Caption = "All demos: done."
End Sub

Sub cmdClear_Click()
    txtOutput.Text = ""
    lblStatus.Caption = "Cleared."
End Sub

Sub cmdDone_Click()
    Unload Me
End Sub
