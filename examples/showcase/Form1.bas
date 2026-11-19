' Form1 — the showcase's main window.
' Every control below is driven from BASIC code; run the project (⌘R)
' and try each one.

Dim Greeted As Long
Dim Added As Long
Dim Seconds As Long

Sub Form_Load()
    Me.Caption = "OxidizedBASIC Control Showcase"
    cboColour.ListIndex = 0
    lblStatus.Caption = "Form_Load ran: the combo box starts on " & cboColour.Text
End Sub

' ---- TextBox, Label, CheckBox, OptionButton -------------------------------
Sub cmdGreet_Click()
    Dim who As String
    who = txtName.Text
    If who = "" Then who = "stranger"
    If optFormal.Value Then
        lblGreeting.Caption = "Good day, " & who & "."
    Else
        lblGreeting.Caption = "Hey " & who & "!"
    End If
    If chkShout.Value = vbChecked Then
        lblGreeting.Caption = UCase(lblGreeting.Caption)
    End If
    Greeted = Greeted + 1
    lblStatus.Caption = "Greeted " & Greeted & " time(s)"
    txtName.SetFocus
End Sub

Sub txtName_Change()
    lblStatus.Caption = "Typing: " & Len(txtName.Text) & " character(s)"
End Sub

Sub chkShout_Click()
    lblStatus.Caption = "Shout is " & IIf(chkShout.Value = vbChecked, "on", "off")
End Sub

Sub optFormal_Click()
    lblStatus.Caption = "Formal style"
End Sub

Sub optCasual_Click()
    lblStatus.Caption = "Casual style"
End Sub

' ---- ComboBox: pick a colour for the picture box ---------------------------
Sub cboColour_Click()
    Select Case cboColour.ListIndex
        Case 0
            picSwatch.BackColor = vbRed
        Case 1
            picSwatch.BackColor = vbGreen
        Case 2
            picSwatch.BackColor = vbBlue
        Case 3
            picSwatch.BackColor = RGB(255, 200, 0)
    End Select
    lblStatus.Caption = "Colour: " & cboColour.Text
End Sub

' ---- ListBox: AddItem, RemoveItem, Clear, ListCount, ListIndex ---------------
Sub cmdAdd_Click()
    Dim fruit As Variant
    fruit = Array("Date", "Elderberry", "Fig", "Grape", "Kiwi", "Lemon")
    lstFruit.AddItem fruit(Added Mod 6)
    Added = Added + 1
    lstFruit.ListIndex = lstFruit.ListCount - 1
    lblStatus.Caption = Plural(lstFruit.ListCount, "fruit") & " in the basket (" & Describe(lstFruit.ListCount) & ")"
End Sub

Sub cmdRemove_Click()
    If lstFruit.ListIndex >= 0 Then
        lstFruit.RemoveItem lstFruit.ListIndex
        lblStatus.Caption = Plural(lstFruit.ListCount, "fruit") & " left"
    Else
        lblStatus.Caption = "Select a fruit to remove first"
    End If
End Sub

Sub cmdClear_Click()
    lstFruit.Clear
    lblStatus.Caption = "The basket is empty"
End Sub

Sub lstFruit_Click()
    lblStatus.Caption = "You picked " & lstFruit.Text
End Sub

' ---- Control array: one handler for all three buttons -----------------------
Sub cmdSwatch_Click(Index As Integer)
    Dim i As Integer
    For i = 0 To 2
        cmdSwatch(i).Font.Bold = (i = Index)
        cmdSwatch(i).Enabled = (i <> Index)
    Next
    cboColour.ListIndex = Index
    lblStatus.Caption = "Swatch " & (Index + 1) & " chosen (Index = " & Index & ")"
End Sub

' ---- Timer -----------------------------------------------------------------
Sub cmdClock_Click()
    tmrClock.Enabled = Not tmrClock.Enabled
    If tmrClock.Enabled Then
        lblClock.Caption = "Clock: running"
    Else
        lblClock.Caption = "Clock: stopped at " & Seconds & "s"
    End If
End Sub

Sub tmrClock_Timer()
    Seconds = Seconds + 1
    lblClock.Caption = "Clock: " & Seconds & "s"
End Sub

' ---- A second form, and End -------------------------------------------------
Sub cmdAbout_Click()
    frmAbout.lblVersion.Caption = "Greetings so far: " & Greeted
    frmAbout.Show
End Sub

Sub cmdQuit_Click()
    End
End Sub
