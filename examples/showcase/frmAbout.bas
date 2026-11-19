' frmAbout — the showcase's second form, opened by Form1's About button.
' It shows what the first form told it, and closes itself with Unload Me.

Sub Form_Load()
    lblNote.Caption = "Form_Load ran before this window opened."
End Sub

Sub cmdClose_Click()
    Unload Me
End Sub

Sub cmdHide_Click()
    ' Hide keeps the form's state; About... shows it again as it was
    Me.Hide
End Sub
