# Keinestamp Task 1 验证门：Lore v0.8.6 能力验证（最终版，四轮实测合并）
# 前置：loreserver 已在运行（无配置 demo 模式，端口 41337/41339）
# 用法：powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify_lore.ps1
$ErrorActionPreference = "Continue"
$lore = "C:\Users\para\AppData\Local\lore\bin\lore.exe"
$base = Join-Path $env:TEMP ("lore_vfy_final_" + $PID)
if (Test-Path $base) { Remove-Item $base -Recurse -Force }
New-Item -ItemType Directory -Path $base | Out-Null
$proj = Join-Path $base "proj"
New-Item -ItemType Directory -Path $proj | Out-Null
Push-Location $proj
function P($name) { Write-Output "`n--- $name ---" }

# --- 1. create / list / status ---
P "1 repository create"
& $lore repository create "lore://127.0.0.1:41337/final$PID" 2>&1 | Out-String
"hello" | Set-Content -Encoding UTF8 a.txt
New-Item -ItemType Directory -Path web | Out-Null
"web-content" | Set-Content -Encoding UTF8 web/b.txt
P "2 status (no scan)"
& $lore status 2>&1 | Out-String
P "3 status --scan (untracked 检测)"
& $lore status --scan 2>&1 | Out-String

# --- 2. commit + metadata + history ---
& $lore stage a.txt 2>&1 | Out-Null
& $lore revision metadata set kind user 2>&1 | Out-Null
P "4 commit with metadata"
& $lore commit "test commit" 2>&1 | Out-String
P "5 metadata get"
& $lore revision metadata get 2>&1 | Out-String
$c1 = (& $lore revision find number 1).Trim()
P "6 history --oneline"
& $lore history 5 --oneline 2>&1 | Out-String
P "7 revision info <sig>"
& $lore revision info $c1 2>&1 | Out-String

# --- 3. 回退语义：branch reset / 顶替 / 撤销回退 ---
"v2" | Set-Content -Encoding UTF8 a.txt
& $lore stage a.txt 2>&1 | Out-Null
& $lore commit "c2" 2>&1 | Out-Null
"v3" | Set-Content -Encoding UTF8 a.txt
& $lore stage a.txt 2>&1 | Out-Null
& $lore commit "c3" 2>&1 | Out-Null
$c3 = (& $lore revision find number 3).Trim()
P "8 branch reset <c1> (回退)"
& $lore branch reset $c1 2>&1 | Out-String
P "9 worktree after rollback: $(Get-Content a.txt)"
P "10 history after rollback"
& $lore history --oneline 2>&1 | Out-String
P "11 commit on top (顶替)"
"v4-supersede" | Set-Content -Encoding UTF8 a.txt
& $lore stage a.txt 2>&1 | Out-Null
& $lore commit "c4 supersede" 2>&1 | Out-String
P "12 history after supersede (c2/c3 应消失)"
& $lore history --oneline 2>&1 | Out-String
P "13 undo rollback: branch reset <c3>"
& $lore branch reset $c3 2>&1 | Out-String
P "14 history after undo (应恢复 c1 c2 c3)"
& $lore history --oneline 2>&1 | Out-String
P "15 worktree after undo: $(Get-Content a.txt)"

# --- 4. revert + 冲突 resolve 闭环 ---
P "16 revert <c1> (冲突场景)"
& $lore revision revert $c1 2>&1 | Out-String
P "17 status conflict"
& $lore status 2>&1 | Out-String
P "18 resolve mine a.txt"
& $lore revision revert resolve mine a.txt 2>&1 | Out-String
P "19 commit after resolve"
& $lore commit "c5 after revert" 2>&1 | Out-String

# --- 5. 导出：dump + file write ---
P "20 repository dump --revision <c1>"
& $lore repository dump --revision $c1 2>&1 | Out-String
$exp = Join-Path $base "export"
New-Item -ItemType Directory -Path $exp | Out-Null
P "21 file write 单文件导出"
& $lore file write --path a.txt --revision $c1 --output (Join-Path $exp "a.txt") 2>&1 | Out-String
P "22 exported content: $(Get-Content (Join-Path $exp 'a.txt'))"

# --- 6. 中文文件名 ---
P "23 chinese filename"
"中文内容" | Set-Content -Encoding UTF8 "角色模型.blend"
& $lore status --scan 2>&1 | Out-String
& $lore stage "角色模型.blend" 2>&1 | Out-Null
& $lore commit "中文提交" 2>&1 | Out-String

# --- 7. push + clone ---
P "24 push"
& $lore push 2>&1 | Out-String
P "25 clone back"
$clone = Join-Path $base "clone"
& $lore clone "lore://127.0.0.1:41337/final$PID" $clone 2>&1 | Out-String

# --- 8. 去重率（6MB × 10 次，每次改末尾 2KB）---
P "26 dedup rate"
New-Item -ItemType Directory -Path (Join-Path $proj "big") | Out-Null
$big = Join-Path $proj "big\scene.blend"
$rnd = New-Object System.Random 42
$bytes = New-Object byte[] (6 * 1024 * 1024)
$rnd.NextBytes($bytes)
[System.IO.File]::WriteAllBytes($big, $bytes)
function RepoSize { (Get-ChildItem -Recurse -Force ".lore" | Measure-Object -Property Length -Sum).Sum }
$s0 = RepoSize
for ($i = 1; $i -le 10; $i++) {
    & $lore stage "big\scene.blend" 2>&1 | Out-Null
    & $lore commit "save $i" 2>&1 | Out-Null
    $fs = [System.IO.File]::Open($big, [System.IO.FileMode]::Open)
    $fs.Seek(-2048, [System.IO.SeekOrigin]::End) | Out-Null
    $buf = New-Object byte[] 2048
    $rnd.NextBytes($buf)
    $fs.Write($buf, 0, 2048)
    $fs.Close()
}
$growth = (RepoSize) - $s0
$full = (Get-Item $big).Length * 10
Write-Output ("repo growth: {0} bytes, 10x full: {1} bytes, dedup rate: {2:P1}" -f $growth, $full, (1 - ($growth / $full)))

Pop-Location
Write-Output "`n===== DONE base=$base ====="
