# ============================================================================
#  retire-windows-postgresql-admin.ps1
#
#  Retire the Windows-native PostgreSQL 18 instance, hand TCP :5432 over to the
#  WSL Ubuntu-22.04 `18/dev` cluster, and make that cluster start with Windows.
#
#  MUST RUN ELEVATED — 停服务 / 卸载 / 注册计划任务都需要管理员令牌。
#
#  Usage (elevated PowerShell):
#      powershell -NoProfile -ExecutionPolicy Bypass -File `
#          "D:\sdkwork-space\sdkwork-webserver\bin\host\retire-windows-postgresql-admin.ps1"
#
#  Optional switches:
#      -KeepInstallDir   保留残留安装目录（默认：卸载后连同 data 目录一并彻底删除）
#      -SkipArchive      不归档物理 data 目录（默认：先 robocopy 归档作安全网）
#      -SkipUninstall    只做停服务 / 归档 / WSL 接管，不跑卸载器
#      -BackupRoot <p>   自定义备份根目录
#
#  回滚：
#    逻辑备份（pg_restore -F c）：
#      <BackupRoot>\sdkwork_ai_dev.dump / sdkwork_deploy_test.dump / postgres.dump
#      <BackupRoot>\globals.sql
#    物理归档：<BackupRoot>\data-physical\
#
#  背景（为什么要退役 Windows 侧）：
#    .wslconfig 设了 networkingMode=mirrored，Windows 与 WSL 共享同一个网络栈。
#    Windows 服务 postgresql-x64-18 开机即绑定 0.0.0.0:5432 + [::]:5432，导致：
#      1) WSL 的 18/dev 集群无法绑定 5432，集群一直是 down；
#      2) 任何连 127.0.0.1:5432 的客户端打到的是 Windows 实例，而该实例里
#         只有一个空壳 sdkwork_ai_dev 库（无匹配角色），认证失败后 sqlx 报出
#         误导性的 "Postgres returned a non-UTF-8 string for its error message"。
# ============================================================================

[CmdletBinding()]
param(
    [string]$InstallDir  = "D:\programs\postgresql",
    [string]$ServiceName = "postgresql-x64-18",
    [string]$BackupRoot  = "D:\sdkwork-space\sdkwork-webserver\.workbuddy\backups\2026-09-15-windows-pg-uninstall",
    [string]$WslDistro   = "Ubuntu-22.04",
    [int]   $WslPgPort   = 5432,
    [switch]$KeepInstallDir,
    [switch]$SkipArchive,
    [switch]$SkipUninstall
)

$ErrorActionPreference = "Stop"

function Step($m) { Write-Host ""; Write-Host "==> $m" -ForegroundColor Cyan }
function Ok($m)   { Write-Host "    [OK]   $m" -ForegroundColor Green }
function Warn($m) { Write-Host "    [WARN] $m" -ForegroundColor Yellow }
function Fail($m) { Write-Host "    [FAIL] $m" -ForegroundColor Red }

# ---------------------------------------------------------------- 0. 提权校验
Step "0/7 管理员权限校验"
$identity  = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Fail "当前不是管理员。请右键『以管理员身份运行 PowerShell』后重试。"
    exit 1
}
Ok "已提权，身份 = $($identity.Name)"

# ---------------------------------------------------------------- 1. 停服务
Step "1/7 停止并禁用 Windows PostgreSQL 服务 ($ServiceName)"
$svc = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if ($null -eq $svc) {
    Warn "服务 $ServiceName 不存在（可能已被卸载），跳过"
} else {
    if ($svc.Status -ne 'Stopped') {
        Stop-Service -Name $ServiceName -Force
        (Get-Service -Name $ServiceName).WaitForStatus('Stopped', '00:00:30')
    }
    Ok "服务已停止（原状态 $($svc.Status)）"
    Set-Service -Name $ServiceName -StartupType Disabled
    Ok "启动类型已置为 Disabled"
}

# 确认 5432 已释放（镜像模式下这一步是后续 WSL 接手的前提）
$still = Get-NetTCPConnection -LocalPort 5432 -State Listen -ErrorAction SilentlyContinue
if ($still) {
    Warn "5432 仍在监听（PID $($still.OwningProcess -join ',')），WSL 可能仍抢不到该端口"
} else {
    Ok "TCP 5432 已释放"
}

# ---------------------------------------------------------------- 2. 归档 data
Step "2/7 物理归档 data 目录（robocopy 备份模式，可读 NetworkService 拥有的文件）"
$dataDir = Join-Path $InstallDir 'data'
$archive = Join-Path $BackupRoot 'data-physical'
if ($SkipArchive) {
    Warn "指定 -SkipArchive，不归档 data 目录"
} elseif (-not (Test-Path $dataDir)) {
    Warn "未找到 $dataDir，跳过归档"
} else {
    New-Item -ItemType Directory -Path $archive -Force | Out-Null
    # /B 用 SeBackupPrivilege 绕过 ACL；robocopy 退出码 <8 均为成功
    robocopy $dataDir $archive /E /B /R:1 /W:1 /NFL /NDL /NP /NJH | Out-Null
    $rc = $LASTEXITCODE
    if ($rc -ge 8) { throw "robocopy 归档失败，退出码 $rc" }
    Ok "已归档到 $archive (robocopy rc=$rc)"
}

# ---------------------------------------------------------------- 3. 卸载
Step "3/7 运行官方卸载器"
$uninstaller = Join-Path $InstallDir 'uninstall-postgresql.exe'
if ($SkipUninstall) {
    Warn "指定了 -SkipUninstall，跳过卸载器"
} elseif (-not (Test-Path $uninstaller)) {
    Warn "找不到卸载器 $uninstaller（可能已卸载）"
} else {
    $proc = Start-Process -FilePath $uninstaller -ArgumentList '--mode','unattended' -Wait -PassThru
    Ok "卸载器退出码 = $($proc.ExitCode)"
}

# ---------------------------------------------------------------- 4. 验证
Step "4/7 验证卸载结果"
$svcAfter = Get-Service -Name $ServiceName -ErrorAction SilentlyContinue
if ($null -eq $svcAfter) {
    Ok "服务 $ServiceName 已注销"
} else {
    Warn "服务仍存在，状态 = $($svcAfter.Status)"
}
$svcKey = "HKLM:\SYSTEM\CurrentControlSet\Services\$ServiceName"
if (Test-Path $svcKey) {
    Remove-Item $svcKey -Recurse -Force -ErrorAction SilentlyContinue
    if (Test-Path $svcKey) { Warn "服务注册表项未能清除：$svcKey" } else { Ok "已清除残留服务注册表项" }
}
# 系统 PATH 里若残留 PostgreSQL bin，移除它（避免遮蔽 WSL 侧的 psql）
$sysPath = [Environment]::GetEnvironmentVariable('Path', 'Machine')
$pgPaths = ($sysPath -split ';') | Where-Object { $_ -match '(?i)postgresql.*\\bin' }
if ($pgPaths.Count -gt 0) {
    $newPath = (($sysPath -split ';') | Where-Object { $_ -notmatch '(?i)postgresql.*\\bin' }) -join ';'
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'Machine')
    Ok "已从系统 PATH 移除：$($pgPaths -join ' | ')"
} else {
    Ok "系统 PATH 无 PostgreSQL 残留"
}

# ---------------------------------------------------------------- 5. WSL 接管 5432
Step "5/7 把 WSL $WslDistro 的 18/dev 集群迁回 :$WslPgPort"
# sed 用 '^port = .*' 匹配，无论原值是 5432 还是 55432 都能改对
wsl.exe -d $WslDistro -u root -- bash -lc "sed -i -E 's/^port[[:space:]]*=.*/port = $WslPgPort/' /etc/postgresql/18/dev/postgresql.conf; grep -n -E '^port' /etc/postgresql/18/dev/postgresql.conf"
wsl.exe -d $WslDistro -u root -- bash -lc "pg_ctlcluster 18 dev restart 2>/dev/null || pg_ctlcluster 18 dev start"
Start-Sleep -Seconds 3
$listen = wsl.exe -d $WslDistro -u root -- bash -lc "ss -lntp | grep ':$WslPgPort' || echo 'NOT-LISTENING'"
Write-Host "    $listen"
if ($listen -match 'NOT-LISTENING') {
    Warn "18/dev 未在 $WslPgPort 上监听，请检查集群日志"
} else {
    Ok "18/dev 已在 $WslPgPort 监听"
}

# ---------------------------------------------------------------- 6. 开机自启
Step "6/7 注册登录自启任务：让 WSL 随 Windows 启动（systemd 随之拉起 18/dev）"
$taskName = "SDKWork WSL $WslDistro Boot"
$action   = New-ScheduledTaskAction -Execute "wsl.exe" -Argument "-d $WslDistro -u root -e true"
$trigger  = New-ScheduledTaskTrigger -AtLogOn -User "$env:USERDOMAIN\$env:USERNAME"
$settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -StartWhenAvailable -ExecutionTimeLimit ([TimeSpan]::Zero)
$taskPrin = New-ScheduledTaskPrincipal -UserId "$env:USERDOMAIN\$env:USERNAME" -LogonType Interactive -RunLevel Limited
Register-ScheduledTask -TaskName $taskName -Action $action -Trigger $trigger -Settings $settings -Principal $taskPrin -Description "Start WSL $WslDistro (systemd) at logon so the PostgreSQL 18/dev cluster owns :$WslPgPort" -Force | Out-Null
Ok "已注册计划任务：$taskName"
Get-ScheduledTask -TaskName $taskName | Select-Object TaskName, State | Format-Table -AutoSize | Out-String | Write-Host

# ---------------------------------------------------------------- 7. 收尾报告
Step "7/7 收尾"
if (Test-Path $InstallDir) {
    $items = (Get-ChildItem $InstallDir -Recurse -Force -ErrorAction SilentlyContinue | Measure-Object).Count
    Warn "$InstallDir 仍有残留（$items 项）"
    if ($KeepInstallDir) {
        Write-Host "        指定了 -KeepInstallDir，保留残留目录。手动清理：" -ForegroundColor DarkGray
        Write-Host "        Remove-Item -Recurse -Force '$InstallDir'" -ForegroundColor DarkGray
    } else {
        Write-Host "        连同 data 目录一并删除…" -ForegroundColor DarkGray
        # data/ 里的文件属于 NT AUTHORITY\NetworkService，先接管所有权再删
        takeown /F $InstallDir /R /D Y 2>&1 | Out-Null
        icacls $InstallDir /grant "*S-1-5-32-544:(F)" /T /C /Q 2>&1 | Out-Null
        Remove-Item $InstallDir -Recurse -Force -ErrorAction SilentlyContinue
        if (Test-Path $InstallDir) {
            Fail "删除失败，请手动处理：$InstallDir"
        } else {
            Ok "已删除 $InstallDir（含 data 目录）"
        }
    }
} else {
    Ok "$InstallDir 已不存在"
}

Write-Host ""
Write-Host "============================================================" -ForegroundColor Cyan
Write-Host " 完成。下一步（无需提权，在普通终端执行）：" -ForegroundColor Cyan
Write-Host "   cd D:\sdkwork-space\sdkwork-webserver" -ForegroundColor White
Write-Host "   pnpm dev" -ForegroundColor White
Write-Host ""
Write-Host " 备份位置（回滚用）：$BackupRoot" -ForegroundColor DarkGray
Write-Host "============================================================" -ForegroundColor Cyan
