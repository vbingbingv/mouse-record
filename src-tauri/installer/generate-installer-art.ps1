# 生成 NSIS 安装向导的品牌图：
#   sidebar.bmp  164x314  欢迎页 / 完成页左侧大图（MUI_WELCOMEFINISHPAGE_BITMAP）
#   header.bmp   150x57   其它页面右上角页眉图（MUI_HEADERIMAGE_BITMAP）
#
# 素材用自带的 src-tauri/icons/128x128.png，换配色后重跑本脚本即可重新生成：
#   pwsh -File src-tauri/installer/generate-installer-art.ps1
#   （或 powershell -ExecutionPolicy Bypass -File ...）
param(
    [string]$Background = "#1E2430",
    [string]$Accent = "#4F8CFF",
    [string]$TitleColor = "#FFFFFF",
    [string]$SubtitleColor = "#98A2B3"
)

Add-Type -AssemblyName System.Drawing

$installerDir = $PSScriptRoot
$logoPath = Join-Path (Split-Path -Parent $installerDir) "icons\128x128.png"

if (-not (Test-Path $logoPath)) {
    throw "找不到图标素材：$logoPath"
}

# 位图按 24bpp 存：NSIS 的 MUI 位图不支持带 alpha 的 32bpp BMP
function New-Canvas([int]$width, [int]$height, $color) {
    $bmp = New-Object System.Drawing.Bitmap($width, $height, [System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::ClearTypeGridFit
    $g.Clear($color)
    return [pscustomobject]@{ Bitmap = $bmp; Graphics = $g }
}

# 中文字体优先用雅黑，缺了就退到英文字体
function New-Font([float]$size, [bool]$bold) {
    $style = if ($bold) { [System.Drawing.FontStyle]::Bold } else { [System.Drawing.FontStyle]::Regular }
    foreach ($name in @("Microsoft YaHei", "Segoe UI", "Arial")) {
        try { return New-Object System.Drawing.Font($name, $size, $style) } catch { }
    }
    return New-Object System.Drawing.Font([System.Drawing.FontFamily]::GenericSansSerif, $size, $style)
}

function Add-CenteredText($graphics, [string]$text, $font, $brush, [float]$y, [float]$width) {
    $format = New-Object System.Drawing.StringFormat
    $format.Alignment = [System.Drawing.StringAlignment]::Center
    $rect = New-Object System.Drawing.RectangleF([float]0, $y, $width, [float]($font.Height + 6))
    $graphics.DrawString($text, $font, $brush, $rect, $format)
    $format.Dispose()
}

$bgColor = [System.Drawing.ColorTranslator]::FromHtml($Background)
$accentColor = [System.Drawing.ColorTranslator]::FromHtml($Accent)
$titleColorValue = [System.Drawing.ColorTranslator]::FromHtml($TitleColor)
$subtitleColorValue = [System.Drawing.ColorTranslator]::FromHtml($SubtitleColor)

$logo = [System.Drawing.Image]::FromFile($logoPath)
$accentBrush = New-Object System.Drawing.SolidBrush($accentColor)
$titleBrush = New-Object System.Drawing.SolidBrush($titleColorValue)
$subtitleBrush = New-Object System.Drawing.SolidBrush($subtitleColorValue)

$titleFont = New-Font 12 $true
$subtitleFont = New-Font 8 $false
$headerFont = New-Font 10 $true

try {
    # 欢迎页 / 完成页左侧大图
    $sidebar = New-Canvas 164 314 $bgColor
    $sidebar.Graphics.FillRectangle($accentBrush, 0, 310, 164, 4)
    $sidebar.Graphics.DrawImage($logo, 34, 56, 96, 96)
    Add-CenteredText $sidebar.Graphics "Mouse Record" $titleFont $titleBrush 186 164
    Add-CenteredText $sidebar.Graphics "鼠标录制与回放" $subtitleFont $subtitleBrush 212 164
    $sidebarPath = Join-Path $installerDir "sidebar.bmp"
    $sidebar.Bitmap.Save($sidebarPath, [System.Drawing.Imaging.ImageFormat]::Bmp)
    $sidebar.Graphics.Dispose()
    $sidebar.Bitmap.Dispose()

    # 内页页眉图
    $header = New-Canvas 150 57 $bgColor
    $header.Graphics.DrawImage($logo, 6, 8, 40, 40)
    $header.Graphics.DrawString("Mouse Record", $headerFont, $titleBrush, 52, 19)
    $headerPath = Join-Path $installerDir "header.bmp"
    $header.Bitmap.Save($headerPath, [System.Drawing.Imaging.ImageFormat]::Bmp)
    $header.Graphics.Dispose()
    $header.Bitmap.Dispose()

    Write-Host "已生成："
    Write-Host "  $sidebarPath"
    Write-Host "  $headerPath"
}
finally {
    $logo.Dispose()
    $accentBrush.Dispose()
    $titleBrush.Dispose()
    $subtitleBrush.Dispose()
    $titleFont.Dispose()
    $subtitleFont.Dispose()
    $headerFont.Dispose()
}
