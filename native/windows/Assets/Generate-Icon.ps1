#requires -Version 5.1
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

function ConvertTo-IconBitmap($Bitmap, [int]$Size) {
    $stream = New-Object System.IO.MemoryStream
    $writer = New-Object System.IO.BinaryWriter $stream
    try {
        $maskStride = [int]([Math]::Ceiling($Size / 32.0) * 4)
        $mask = New-Object byte[] ($maskStride * $Size)
        $writer.Write([uint32]40)
        $writer.Write([int32]$Size)
        $writer.Write([int32]($Size * 2))
        $writer.Write([uint16]1)
        $writer.Write([uint16]32)
        $writer.Write([uint32]0)
        $writer.Write([uint32]($Size * $Size * 4 + $mask.Length))
        foreach ($unused in 1..4) { $writer.Write([int32]0) }
        for ($y = $Size - 1; $y -ge 0; $y--) {
            for ($x = 0; $x -lt $Size; $x++) {
                $color = $Bitmap.GetPixel($x, $y)
                $writer.Write([byte]$color.B)
                $writer.Write([byte]$color.G)
                $writer.Write([byte]$color.R)
                $writer.Write([byte]$color.A)
                if ($color.A -eq 0) {
                    $index = ($Size - 1 - $y) * $maskStride + [int][Math]::Floor($x / 8.0)
                    $mask[$index] = $mask[$index] -bor (0x80 -shr ($x % 8))
                }
            }
        }
        $writer.Write([byte[]]$mask)
        return ,$stream.ToArray()
    } finally {
        $writer.Dispose()
        $stream.Dispose()
    }
}

$canvas = New-Object System.Drawing.Bitmap 1024, 1024
$graphics = [System.Drawing.Graphics]::FromImage($canvas)
$graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$graphics.ScaleTransform(4, 4)
$tile = New-Object System.Drawing.Drawing2D.GraphicsPath
$heart = New-Object System.Drawing.Drawing2D.GraphicsPath
$gradient = New-Object System.Drawing.Drawing2D.LinearGradientBrush (
    [System.Drawing.Point]::new(0, 0)),
    ([System.Drawing.Point]::new(256, 256)),
    ([System.Drawing.ColorTranslator]::FromHtml('#18BFA9')),
    ([System.Drawing.ColorTranslator]::FromHtml('#0D6F70'))
$pulse = New-Object System.Drawing.Pen ([System.Drawing.ColorTranslator]::FromHtml('#095F66')), 13
$pulse.StartCap = $pulse.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
$pulse.LineJoin = [System.Drawing.Drawing2D.LineJoin]::Round
$frames = @()
try {
    $tile.AddArc(4, 4, 112, 112, 180, 90)
    $tile.AddArc(140, 4, 112, 112, 270, 90)
    $tile.AddArc(140, 140, 112, 112, 0, 90)
    $tile.AddArc(4, 140, 112, 112, 90, 90)
    $tile.CloseFigure()
    $graphics.FillPath($gradient, $tile)
    $heart.AddBezier(128, 211, 108, 194, 46, 151, 40, 112)
    $heart.AddBezier(40, 112, 30, 55, 94, 38, 128, 80)
    $heart.AddBezier(128, 80, 162, 38, 226, 55, 216, 112)
    $heart.AddBezier(216, 112, 210, 151, 148, 194, 128, 211)
    $heart.CloseFigure()
    $graphics.FillPath([System.Drawing.Brushes]::White, $heart)
    $points = @(
        [System.Drawing.PointF]::new(59, 130),
        [System.Drawing.PointF]::new(89, 130),
        [System.Drawing.PointF]::new(107, 105),
        [System.Drawing.PointF]::new(129, 159),
        [System.Drawing.PointF]::new(151, 121),
        [System.Drawing.PointF]::new(165, 130),
        [System.Drawing.PointF]::new(196, 130)
    )
    $graphics.DrawLines($pulse, [System.Drawing.PointF[]]$points)
    foreach ($size in @(16, 24, 32, 48, 64, 128, 256)) {
        $bitmap = New-Object System.Drawing.Bitmap $size, $size
        $scaled = [System.Drawing.Graphics]::FromImage($bitmap)
        $stream = New-Object System.IO.MemoryStream
        try {
            $scaled.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
            $scaled.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
            $scaled.DrawImage($canvas, 0, 0, $size, $size)
            $bitmap.Save($stream, [System.Drawing.Imaging.ImageFormat]::Png)
            # Keep classic DIB frames for small icons and PNG compression for 256px.
            $bytes = if ($size -eq 256) { $stream.ToArray() } else { ConvertTo-IconBitmap $bitmap $size }
            $frames += [pscustomobject]@{ Size = $size; Bytes = [byte[]]$bytes }
            if ($size -eq 256) {
                $bitmap.Save((Join-Path $PSScriptRoot 'Health.png'), [System.Drawing.Imaging.ImageFormat]::Png)
            }
        } finally {
            $stream.Dispose()
            $scaled.Dispose()
            $bitmap.Dispose()
        }
    }
    $file = [System.IO.File]::Create((Join-Path $PSScriptRoot 'Health.ico'))
    $writer = New-Object System.IO.BinaryWriter $file
    try {
        $writer.Write([uint16]0)
        $writer.Write([uint16]1)
        $writer.Write([uint16]$frames.Count)
        $offset = 6 + 16 * $frames.Count
        foreach ($frame in $frames) {
            $dimension = if ($frame.Size -eq 256) { 0 } else { $frame.Size }
            $writer.Write([byte]$dimension)
            $writer.Write([byte]$dimension)
            $writer.Write([byte]0)
            $writer.Write([byte]0)
            $writer.Write([uint16]1)
            $writer.Write([uint16]32)
            $writer.Write([uint32]$frame.Bytes.Length)
            $writer.Write([uint32]$offset)
            $offset += $frame.Bytes.Length
        }
        foreach ($frame in $frames) { $writer.Write([byte[]]$frame.Bytes) }
    } finally {
        $writer.Dispose()
        $file.Dispose()
    }
} finally {
    $pulse.Dispose()
    $gradient.Dispose()
    $heart.Dispose()
    $tile.Dispose()
    $graphics.Dispose()
    $canvas.Dispose()
}
Write-Host 'Generated original Health artwork: 16-256px Windows icon and 256px PNG.'
