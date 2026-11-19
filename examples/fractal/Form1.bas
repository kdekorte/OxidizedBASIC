' Fractal -- the Mandelbrot set, drawn one pixel at a time with PSet.
'
' Ported from a QB64 demo (SCREEN _NEWIMAGE(640, 480, 256) and PSET on the
' screen). Here the screen is picFractal, a 640 x 480 picture box, and the
' 256-colour palette index becomes an RGB colour from Pal().
'
' Press Draw. DoEvents shows the picture as it grows, a row of pixels at a
' time, and lets Stop (or closing the window) end a draw in progress.

Sub Form_Load()
    lblStatus.Caption = "Press Draw."
End Sub

Sub cmdDraw_Click()
    DrawFractal 640, 480
End Sub

' Draw the Mandelbrot set over a pw x ph pixel picture.
Sub DrawFractal(pw As Long, ph As Long)
    Dim maxiter As Long
    Dim x As Long, y As Long, k As Long, n As Long
    Dim rmin As Double, rmax As Double, imin As Double, imax As Double
    Dim dr As Double, di As Double
    Dim cr As Double, ci As Double
    Dim zr As Double, zi As Double, zr2 As Double, zi2 As Double

    cmdDraw.Enabled = False
    picFractal.Cls
    lblStatus.Caption = "Generating Mandelbrot fractal..."

    maxiter = 128
    rmin = -2.0: rmax = 0.8
    imin = -1.05: imax = 1.05
    dr = (rmax - rmin) / pw
    di = (imax - imin) / ph

    For y = 0 To ph - 1
        ci = imin + y * di
        For x = 0 To pw - 1
            cr = rmin + x * dr
            zr = 0.0: zi = 0.0
            zr2 = 0.0: zi2 = 0.0
            k = 0
            While (zr2 + zi2 <= 4.0) And (k < maxiter)
                zi = 2.0 * zr * zi + ci
                zr = zr2 - zi2 + cr
                zr2 = zr * zr
                zi2 = zi * zi
                k = k + 1
            Wend
            If k = maxiter Then n = 0 Else n = (k * 3 + 16) Mod 256
            picFractal.PSet (x, y), Pal(n)
        Next x
        ' show the rows drawn so far, now and then
        If y Mod 8 = 7 Then
            lblStatus.Caption = "Row " & (y + 1) & " of " & ph
            DoEvents
        End If
    Next y

    lblStatus.Caption = "Mandelbrot set (" & pw & " x " & ph & ", 256 colours)"
    cmdDraw.Enabled = True
End Sub

' The colour for palette index n (0 to 255): black for 0, which is inside
' the set, and a ramp through the colours of the rainbow for the rest.
Function Pal(n As Long) As Long
    Dim seg As Long, f As Long
    If n = 0 Then
        Pal = RGB(0, 0, 0)
        Exit Function
    End If
    seg = (n \ 43) Mod 6
    f = (n Mod 43) * 6
    Select Case seg
        Case 0
            Pal = RGB(255, f, 0)
        Case 1
            Pal = RGB(255 - f, 255, 0)
        Case 2
            Pal = RGB(0, 255, f)
        Case 3
            Pal = RGB(0, 255 - f, 255)
        Case 4
            Pal = RGB(f, 0, 255)
        Case Else
            Pal = RGB(255, 0, 255 - f)
    End Select
End Function
