' frmTabs — a TabControl with three pages. Next and Back move between them;
' a click on a tab also changes the page (its Click). Status says which page.

Sub Form_Load()
    ShowPage
End Sub

Sub tabSettings_Click()
    ShowPage
End Sub

Sub ShowPage()
    lblStatus.Caption = "Page " & (tabSettings.Tab + 1) & " of " & tabSettings.Tabs & ": " & tabSettings.TabCaption
End Sub

Sub cmdNext_Click()
    If tabSettings.Tab < tabSettings.Tabs - 1 Then
        tabSettings.Tab = tabSettings.Tab + 1
    End If
    ShowPage
End Sub

Sub cmdBack_Click()
    If tabSettings.Tab > 0 Then
        tabSettings.Tab = tabSettings.Tab - 1
    End If
    ShowPage
End Sub

Sub cmdClose_Click()
    Unload Me
End Sub
