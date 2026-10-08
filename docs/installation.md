# Installation

## Download

Get the installer for your OS from the [latest release](https://github.com/hadihaider055/catalyst-dbench/releases/latest):

| OS                    | File                                    |
| --------------------- | --------------------------------------- |
| macOS (Apple Silicon) | `Catalyst.DBench_<version>_aarch64.dmg` |
| macOS (Intel)         | `Catalyst.DBench_<version>_x64.dmg`     |
| Windows               | `.msi` or `-setup.exe`                  |
| Linux                 | `.AppImage`, `.deb` or `.rpm`           |

The app isn't signed with Apple or Microsoft certificate yet, so your OS will ask you to confirm it the first time you open it.

**macOS:** the first launch says _"cannot be opened because the developer cannot be verified"_. Right-click the app → **Open** → **Open**, or go to **System Settings → Privacy & Security → Open Anyway**. You only need to do this once. If you get _"is damaged and can't be opened"_ instead, clear the download flag:

```bash
xattr -cr "/Applications/Catalyst DBench.app"
```

**Windows:** if SmartScreen shows _"Windows protected your PC"_, click **More info → Run anyway**.

**Linux (AppImage):**

```bash
chmod +x Catalyst*.AppImage
./Catalyst*.AppImage
```

## Oracle Instant Client (Oracle only)

The Oracle driver loads Oracle's client libraries at runtime. Every other database works without them.

| OS      | Install                                                                                                                                                                       |
| ------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS   | Download **Instant Client Basic** from [Oracle](https://www.oracle.com/database/technologies/instant-client/downloads.html), unzip it, and copy the `.dylib` files to `~/lib` |
| Linux   | Install the Instant Client Basic package, then run `sudo ldconfig` (or set `LD_LIBRARY_PATH`)                                                                                 |
| Windows | Unzip Instant Client Basic and add the folder to your `PATH`                                                                                                                  |

If it's missing, connecting shows error `DPI-1047` along with a link to the download page.

## Build from source

**Prerequisites**

- [Rust](https://rustup.rs) 1.92 or newer (`rustup update`)
- [Node.js](https://nodejs.org) 20+
- Tauri CLI: `cargo install tauri-cli --version "^2" --locked`
- Platform dependencies:
  - **macOS:** `xcode-select --install`
  - **Linux (Debian/Ubuntu):** `sudo apt install build-essential libwebkit2gtk-4.1-dev libgtk-3-dev libappindicator3-dev librsvg2-dev libssl-dev patchelf`
  - **Windows:** [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) and WebView2 (preinstalled on Windows 11)

**Run in development** (two terminals):

```bash
git clone https://github.com/hadihaider055/catalyst-dbench.git
cd catalyst-dbench

# Terminal 1: frontend (Vite on http://localhost:5173)
cd frontend && npm install && npm run dev

# Terminal 2: desktop app
cd crates/dbench-app && cargo tauri dev
```

**Build an installer:**

```bash
cd frontend && npm install && cd ..
cd crates/dbench-app && cargo tauri build
# Output: target/release/bundle/ (at the repo root)
```

## Try it with local databases

No database handy? Start one with Docker, then connect to `localhost` in the app:

```bash
docker run -d -p 5432:5432   -e POSTGRES_PASSWORD=postgres postgres:16
docker run -d -p 3306:3306   -e MYSQL_ROOT_PASSWORD=root mysql:8
docker run -d -p 27017:27017 mongo:7
docker run -d -p 6379:6379   redis:7
docker run -d -p 1433:1433   -e ACCEPT_EULA=Y -e MSSQL_SA_PASSWORD='Dbench!2024' mcr.microsoft.com/mssql/server:2022-latest
docker run -d -p 1521:1521   -e ORACLE_PASSWORD=oracle gvenzl/oracle-free:slim
docker run -d -p 9200:9200   -e discovery.type=single-node -e DISABLE_SECURITY_PLUGIN=true opensearchproject/opensearch:2
docker run -d -p 8000:8000   surrealdb/surrealdb:latest start --user root --pass root
docker run -d -p 8001:8000   amazon/dynamodb-local   # endpoint: http://localhost:8001
```

Local containers use self-signed certificates or none at all, so turn **TLS off** for them. SQLite needs nothing; just pick a `.db` file.
