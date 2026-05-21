# Windows Bootstrap

The recommended Windows setup for Librarian is:

- native Windows Rust toolchain;
- MSYS2/UCRT GCC for the open GNU Rust target;
- Podman CLI for containers;
- Podman machine backed by WSL2;
- no Docker Desktop requirement.

This keeps the root orchestrator native to Windows while running Linux
containers inside a small WSL2-backed VM.

## Recommended: Podman

```powershell
.\scripts\bootstrap-windows.ps1
```

The script installs:

- Rustup;
- MSYS2;
- the Rust `stable-x86_64-pc-windows-gnu` toolchain;
- UCRT64 GCC and pkgconf;
- Podman;
- a rootless Podman machine;
- a debug `librarian.exe`;
- `dist\windows-x64\librarian.ps1`, a portable launcher that stores state in
  `dist\windows-x64\.librarian`;
- the `librarian-agent:latest` image when `-BuildAgentImage` is passed.

Podman forwards a Docker-compatible API to `npipe:////./pipe/docker_engine`, so
Docker API clients can usually connect without `DOCKER_HOST`.

If the Windows Podman API forwarding is broken but the WSL-backed Podman
machine works, use the WSL fallback:

```powershell
.\scripts\bootstrap-windows.ps1 -Runtime wsl-podman
```

For a silent default setup rooted in `%APPDATA%\Librarian`:

```powershell
cargo +stable-x86_64-pc-windows-gnu run -- setup --yes
```

For a portable test root:

```powershell
cargo +stable-x86_64-pc-windows-gnu run -- setup --root .\.librarian-test
```

## Alternative: Rancher Desktop

Rancher Desktop is open source and can run either containerd or dockerd/moby.
For Librarian, choose the dockerd/moby engine if you want Docker CLI and Docker
API compatibility.

Install:

```powershell
winget install --id SUSE.RancherDesktop --exact --source winget
```

## Alternative: Docker Engine Inside WSL2

This avoids Docker Desktop and Podman Desktop entirely, but it is best when the
Librarian process also runs inside WSL2. If Librarian runs as a native Windows
process, you must expose or bridge the Docker API from WSL, which adds security
and setup complexity.

Use this mode later if we decide to ship a Linux-first self-contained runtime.

## Not Recommended For This Project

- Docker Desktop: good product, but heavier and not necessary for the local OSS
  baseline.
- Colima/Lima: good on macOS/Linux, not the native Windows path.
- Raw containerd/nerdctl on Windows: possible through WSL2, but less ergonomic
  for the Windows host process.
