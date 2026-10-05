# Grabs the primary screen (works for Elden Ring in borderless mode) into a PNG, plus a smaller copy.
#   .\tools\screenshot.ps1 out.png [-Scale 0.4]
param([string]$Out = "$PSScriptRoot\..\test-output\screen.png", [double]$Scale = 0.4)
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
$b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$w = [int]($b.Width * $Scale); $h = [int]($b.Height * $Scale)
$small = New-Object System.Drawing.Bitmap $bmp, $w, $h
$smallPath = [System.IO.Path]::ChangeExtension($Out, $null).TrimEnd('.') + "-small.png"
$small.Save($smallPath, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose(); $small.Dispose()
$smallPath
