# lesson:daemon:autostart-lock-race

**Tier**: reference  
**Entry type**: lesson  
**Date**: 2026-03-29

## Operational Note

`touring-hook --start-daemon` IS the daemon process (runs `run_daemon_async()` in-process).

`try_autostart_daemon()` spawns `touring-hook --start-daemon` as the background daemon.

## Lock File

Lock file at `/tmp/touring-daemon-{uid}.lock` is written with the spawned PID.

## Race Condition

When killing the daemon, also remove the lock file before restarting to avoid:

> "Another touring-daemon is already running"

This false positive occurs when the lock file remains after the daemon process was killed.

## Fix Protocol

```bash
kill <daemon-pid>
rm /tmp/touring-daemon-$(id -u).lock
# now safe to restart
touring-hook --start-daemon &
```
