# 解析 PE 导入表，列出 exe/dll 依赖的 DLL 清单（兼容 PowerShell 5.1）
param([Parameter(Mandatory=$true)][string]$Path)

$bytes = [System.IO]::ReadAllBytes($Path)
$peOff = [BitConverter]::ToInt32($bytes, 0x3C)
$magic = [BitConverter]::ToUInt16($bytes, $peOff + 0x18)
$optOff = $peOff + 0x18
$is64 = ($magic -eq 0x20B)
$dataDirOff = $optOff + 112
if (-not $is64) { $dataDirOff = $optOff + 96 }
$importDirRva = [BitConverter]::ToUInt32($bytes, $dataDirOff + 8)
$importDirSize = [BitConverter]::ToUInt32($bytes, $dataDirOff + 12)
if ($importDirRva -eq 0) { Write-Output "no imports"; exit }

$numSections = [BitConverter]::ToUInt16($bytes, $peOff + 6)
$secOff = $optOff + [BitConverter]::ToUInt16($bytes, $peOff + 0x14)
function RvaToOff([uint32]$rva) {
    for ($i = 0; $i -lt $numSections; $i++) {
        $s = $secOff + $i * 40
        $vsize = [BitConverter]::ToUInt32($bytes, $s + 8)
        $vaddr = [BitConverter]::ToUInt32($bytes, $s + 12)
        $rawOff = [BitConverter]::ToUInt32($bytes, $s + 20)
        if ($rva -ge $vaddr -and $rva -lt ($vaddr + $vsize)) { return $rawOff + ($rva - $vaddr) }
    }
    return -1
}

function ReadCStr([int]$off) {
    $end = $off; while ($bytes[$end] -ne 0) { $end++ }
    return [System.Text.Encoding]::ASCII.GetString($bytes, $off, $end - $off)
}

$machine = [BitConverter]::ToUInt16($bytes, $peOff + 4)
Write-Output "machine=$machine (8664=x64, 4664=arm64) imports=$importDirSize bytes"
$desc = RvaToOff $importDirRva
while ($true) {
    $nameRva = [BitConverter]::ToUInt32($bytes, $desc + 12)
    if ($nameRva -eq 0) { break }
    $dll = ReadCStr (RvaToOff $nameRva)
    $thunkRva = [BitConverter]::ToUInt32($bytes, $desc + 16)
    $t = RvaToOff $thunkRva; $count = 0
    while ([BitConverter]::ToUInt64($bytes, $t) -ne 0) { $count++; $t += 8 }
    Write-Output "$dll ($count funcs)"
    $desc += 20
}
