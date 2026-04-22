#!/usr/bin/env bash
set -e

# =========================
# CONFIG
# =========================

PREFIX="$HOME/opt/cross"
TARGET=x86_64-elf

BINUTILS_VER=2.44
GCC_VER=15.1.0
GDB_VER=16.3

RUSTUP_URL="https://sh.rustup.rs"
RUST_TARGET="x86_64-unknown-none"

BINUTILS_URL="https://ftp.gnu.org/gnu/binutils/binutils-$BINUTILS_VER.tar.gz"
GCC_URL="https://ftp.gnu.org/gnu/gcc/gcc-$GCC_VER/gcc-$GCC_VER.tar.gz"
GDB_URL="https://ftp.gnu.org/gnu/gdb/gdb-$GDB_VER.tar.gz"

BUILD_DIR="$HOME/.cache/osdev-toolchain/$TARGET-$GCC_VER"

# =========================
# ENVIRONMENT DETECTION
# =========================

# Detect WSL (both WSL1 and WSL2)
IS_WSL=0
if grep -qiE "(microsoft|wsl)" /proc/version 2>/dev/null; then
    IS_WSL=1
    echo "==> Detected WSL environment"
fi

# On WSL, cap jobs to avoid OOM during heavy GCC optimization passes.
# GCC's bootstrap-O3 can spike RAM per-job significantly.
if [ "$IS_WSL" -eq 1 ]; then
    # Use at most 75% of logical CPUs, minimum 1
    JOBS=$(( ($(nproc) * 3 / 4 > 0) ? $(nproc) * 3 / 4 : 1 ))
else
    JOBS=$(nproc)
fi

# =========================
# BUILD OPTIMIZATION FLAGS
# =========================

# These flags apply to the HOST build of the toolchain itself (not your OS).
# -O2 -pipe: reasonable optimization, avoid slow temp files
# -fomit-frame-pointer: squeeze a bit more out of the host build
# -march=native: use every instruction your CPU supports for the host build
HOST_CFLAGS="-O2 -pipe -march=native -fomit-frame-pointer"
HOST_CXXFLAGS="$HOST_CFLAGS"

export CFLAGS_FOR_BUILD="$HOST_CFLAGS"
export CXXFLAGS_FOR_BUILD="$HOST_CXXFLAGS"

# bootstrap-O3 runs heavier optimization passes that can OOM on WSL
# (WSL memory is shared with Windows and often has no swap by default).
# Fall back to the standard bootstrap-O2 there.
if [ "$IS_WSL" -eq 1 ]; then
    GCC_BUILD_CONFIG="bootstrap-O2"
    echo "==> WSL detected: using bootstrap-O2 to avoid OOM (add swap or use native Linux for bootstrap-O3)"
else
    GCC_BUILD_CONFIG="bootstrap-O3"
fi

# =========================
# ENV
# =========================

export PATH="$PREFIX/bin:$PATH"

mkdir -p "$PREFIX"
mkdir -p "$BUILD_DIR"

echo "==> Using PREFIX: $PREFIX"
echo "==> Build dir: $BUILD_DIR"
echo "==> Target: $TARGET"
echo "==> Jobs: $JOBS"
echo "==> GCC build config: $GCC_BUILD_CONFIG"

# =========================
# DEPENDENCIES
# =========================

echo "==> Checking dependencies..."

REQUIRED=(wget curl tar make gcc g++ bison flex nasm mcopy mkfs.fat qemu-system-x86_64 parted lsb_release gpg)

for cmd in "${REQUIRED[@]}"; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "Missing dependency: $cmd"
        echo ""
        echo "Install on Debian/Ubuntu:"
        echo "  sudo apt install -y build-essential bison flex texinfo \\"
        echo "      libgmp3-dev libmpc-dev libmpfr-dev libisl-dev nasm wget curl tar \\"
        echo "      mtools dosfstools qemu-system-x86 parted \\"
        echo "      lsb-release gnupg apt-transport-https"
        exit 1
    fi
done

# =========================
# CLANG-FORMAT
# =========================

LLVM_VER=20   # latest stable — bump this when LLVM releases a new version

echo "==> Checking clang-format..."

if command -v clang-format >/dev/null 2>&1; then
    echo "==> clang-format already installed: $(clang-format --version)"
elif ! command -v apt-get >/dev/null 2>&1; then
    echo "    WARNING: apt-get not found. Install clang-format manually:"
    echo "      https://apt.llvm.org  or  your distro's package manager"
else
    echo "==> Installing clang-format-$LLVM_VER via LLVM apt repository..."


    # lsb_release and gpg guaranteed present by dep check above
    CODENAME=$(lsb_release -cs)

    # Add the LLVM apt signing key and repo (idempotent)
    LLVM_KEY=/usr/share/keyrings/llvm-archive-keyring.gpg
    if [ ! -f "$LLVM_KEY" ]; then
        curl -fsSL https://apt.llvm.org/llvm-snapshot.gpg.key \
            | sudo gpg --dearmor -o "$LLVM_KEY"
    fi

    LLVM_LIST=/etc/apt/sources.list.d/llvm.list
    if [ ! -f "$LLVM_LIST" ]; then
        echo "deb [signed-by=$LLVM_KEY] https://apt.llvm.org/$CODENAME/ llvm-toolchain-$CODENAME-$LLVM_VER main" \
            | sudo tee "$LLVM_LIST" > /dev/null
    fi

    sudo apt-get update -qq
    sudo apt-get install -y "clang-format-$LLVM_VER"

    # Create an unversioned symlink so editors and CI find it as plain clang-format
    sudo ln -sf "/usr/bin/clang-format-$LLVM_VER" /usr/local/bin/clang-format
    echo "==> clang-format installed: $(clang-format --version)"
fi

# =========================
# RUST
# =========================

echo "==> Checking Rust..."

if ! command -v cargo >/dev/null 2>&1; then
    echo "==> Rust not found. Installing..."
    curl --proto '=https' --tlsv1.2 -sSf "$RUSTUP_URL" | sh -s -- -y
    source "$HOME/.cargo/env"
else
    echo "==> Rust already installed"
fi

echo "==> Updating Rust..."
rustup update

echo "==> Installing components..."
rustup component add rustfmt clippy rust-src llvm-tools

echo "==> Adding OS target..."
rustup target add "$RUST_TARGET"

echo "==> Installing dev-deps..."
cargo install tokei asmfmt


# =========================
# PARALLEL DOWNLOAD HELPER
# =========================

# Kick off downloads in the background so they overlap with each other.
# We only download what we don't already have.
echo "==> Pre-fetching source tarballs in parallel..."
cd "$BUILD_DIR"

download_if_missing() {
    local url="$1"
    local file="$2"
    if [ ! -f "$file" ]; then
        echo "    Downloading $file..."
        wget -q --compression=auto -O "$file" "$url" &
    fi
}

download_if_missing "$BINUTILS_URL" "binutils-$BINUTILS_VER.tar.gz"
download_if_missing "$GCC_URL"      "gcc-$GCC_VER.tar.gz"
download_if_missing "$GDB_URL"      "gdb-$GDB_VER.tar.gz"

# Wait for all background downloads to finish before proceeding
wait
echo "==> All tarballs ready."

# =========================
# BINUTILS
# =========================

echo "==> Checking binutils..."

if [ -f "$PREFIX/bin/$TARGET-as" ]; then
    echo "==> Binutils already installed"
else
    echo "==> Building binutils..."

    cd "$BUILD_DIR"
    [ -d "binutils-$BINUTILS_VER" ] || tar -xf "binutils-$BINUTILS_VER.tar.gz"

    mkdir -p "binutils-$BINUTILS_VER/build"
    cd "binutils-$BINUTILS_VER/build"

    ../configure \
        --target=$TARGET \
        --prefix="$PREFIX" \
        --with-sysroot \
        --disable-nls \
        --disable-werror \
        --enable-lto \
        --enable-plugins \
        --enable-gold \
        --disable-multilib

    make -j"$JOBS"
    make -j"$JOBS" install
fi

# =========================
# GCC
# =========================

echo "==> Checking GCC..."

if [ -f "$PREFIX/bin/$TARGET-gcc" ]; then
    echo "==> GCC already installed"
else
    echo "==> Building GCC..."

    cd "$BUILD_DIR"
    [ -d "gcc-$GCC_VER" ] || tar -xf "gcc-$GCC_VER.tar.gz"

    cd "gcc-$GCC_VER"

    if [ ! -d "gmp" ]; then
        set +e
        ./contrib/download_prerequisites > /dev/null
        set -e
    fi

    mkdir -p build
    cd build

    ../configure \
        --target=$TARGET \
        --prefix="$PREFIX" \
        --disable-nls \
        --enable-languages=c \
        --without-headers \
        --disable-shared \
        --disable-threads \
        --disable-multilib \
        --disable-bootstrap \
        --enable-lto \
        --enable-checking=release \
        --with-tune=native \
        --with-build-config=$GCC_BUILD_CONFIG

    make all-gcc -j"$JOBS"
    make all-target-libgcc -j"$JOBS"
    make install-gcc
    make install-target-libgcc
fi

# =========================
# GDB
# =========================

echo "==> Checking GDB..."

if [ -f "$PREFIX/bin/$TARGET-gdb" ]; then
    echo "==> GDB already installed"
else
    echo "==> Building GDB..."

    cd "$BUILD_DIR"
    [ -d "gdb-$GDB_VER" ] || tar -xf "gdb-$GDB_VER.tar.gz"

    mkdir -p "gdb-$GDB_VER/build"
    cd "gdb-$GDB_VER/build"

    ../configure \
        --target=$TARGET \
        --prefix="$PREFIX" \
        --disable-nls \
        --disable-werror

    make -j"$JOBS"
    make -j"$JOBS" install
fi

# =========================
# CLEANUP
# =========================

echo "==> Cleaning build cache..."
rm -rf "$BUILD_DIR"

# =========================
# PATH SETUP
# =========================

echo "==> Setting up PATH..."

LINE='export PATH="$HOME/opt/cross/bin:$PATH"'

add_to_file() {
    local FILE="$1"
    if [ -f "$FILE" ]; then
        if ! grep -Fxq "$LINE" "$FILE"; then
            echo "$LINE" >> "$FILE"
            echo "Added to $FILE"
        else
            echo "Already in $FILE"
        fi
    fi
}

add_to_file "$HOME/.bashrc"
add_to_file "$HOME/.zshrc"

export PATH="$HOME/opt/cross/bin:$PATH"

# =========================
# DONE
# =========================

echo ""
echo "=============================="
echo " TOOLCHAIN READY"
echo "=============================="
echo ""
echo "x86_64-elf-gcc is now available."
echo "Restart your shell or run: source ~/.bashrc"