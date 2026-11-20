' frmFiles — browse the volumes, a volume's folders, and the files in a folder.
' Choosing a file shows its full path (FileName).

Sub volVolume_Click()
    If volVolume.ListIndex >= 0 Then
        dirFolder.Path = "/Volumes/" & volVolume.Text
        lblChosen.Caption = "Folders on " & volVolume.Text
    End If
End Sub

Sub dirFolder_Change()
    filFile.Path = dirFolder.Path
    lblChosen.Caption = "Folder " & dirFolder.Path
End Sub

Sub filFile_Click()
    lblChosen.Caption = "Chosen: " & filFile.FileName
End Sub

Sub cmdDone_Click()
    Unload Me
End Sub
