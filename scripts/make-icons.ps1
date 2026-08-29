# 生成占位应用图标:蓝色圆形徽章 + 白色向上箭头(站起来)。
# 用法:powershell -NoProfile -ExecutionPolicy Bypass -File scripts/make-icons.ps1
# 产出:src-tauri/icons/icon.ico 与 icon.png。替换为正式图标时重跑即可。

Add-Type -AssemblyName System.Drawing

$dir = Join-Path $PSScriptRoot "..\src-tauri\icons"
New-Item -ItemType Directory -Force $dir | Out-Null

function New-IconBitmap([int]$size) {
    $bmp = New-Object System.Drawing.Bitmap $size, $size
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.Clear([System.Drawing.Color]::Transparent)

    $bg = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(255, 56, 132, 246))
    $g.FillEllipse($bg, 0, 0, $size - 1, $size - 1)

    $pen = New-Object System.Drawing.Pen ([System.Drawing.Color]::White), ($size * 0.09)
    $pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
    $c = $size / 2
    $g.DrawLine($pen, $c, $size * 0.72, $c, $size * 0.30)
    $g.DrawLine($pen, $c - $size * 0.18, $size * 0.46, $c, $size * 0.28)
    $g.DrawLine($pen, $c + $size * 0.18, $size * 0.46, $c, $size * 0.28)

    $g.Dispose()
    return $bmp
}

$bmp = New-IconBitmap 32
$icon = [System.Drawing.Icon]::FromHandle($bmp.GetHicon())
$fs = [System.IO.File]::Create((Join-Path $dir "icon.ico"))
$icon.Save($fs)
$fs.Dispose()
$bmp.Save((Join-Path $dir "icon.png"), [System.Drawing.Imaging.ImageFormat]::Png)
$icon.Dispose()
$bmp.Dispose()

Write-Host "icons written to $dir"
