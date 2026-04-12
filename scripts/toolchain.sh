#!/usr/bin/env bash
set -e

# =========================
# CONFIG
# =========================
PREFIX="$HOME/opt/cross"
TARGET=x86_64-elf
JOBS=$(nproc --ignore=1)

BINUTILS_VER=2.46.0
GCC_VER=15.2.0
GDB_VER=17.1

RUSTUP_URL="https://sh.rustup.rs"
RUST_TARGET="x86_64-unknown-none"

BINUTILS_URL="https://ftp.gnu.org/gnu/binutils/binutils-$BINUTILS_VER.tar.gz"
GCC_URL="https://ftp.gnu.org/gnu/gcc/gcc-$GCC_VER/gcc-$GCC_VER.tar.gz"
GDB_URL="https://ftp.gnu.org/gnu/gdb/gdb-$GDB_VER.tar.gz"

# =========================
# ENV
# =========================
export PATH="$PREFIX/bin:$PATH"

mkdir -p "$PREFIX"
mkdir -p build

echo "==> Using PREFIX: $PREFIX"
echo "==> Target: $TARGET"
echo "==> Jobs: $JOBS"

# =========================
# DEPENDENCIES Install
# =========================
echo "==> Checking dependencies..."

REQUIRED=(wget tar make gcc g++ bison flex nasm mcopy mkfs.fat qemu-system-x86_64 parted)

for cmd in "${REQUIRED[@]}"; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "Missing dependency: $cmd"
        echo ""
        echo "Install on Debian/Ubuntu:"
        echo "sudo apt install -y build-essential bison flex texinfo \\"
        echo "    libgmp3-dev libmpc3 libmpfr-dev libisl-dev nasm wget tar \\"
        echo "    mtools dosfstools qemu-system-x86 parted"
        exit 1
    fi
done

# =========================
# Rust Install
# =========================
echo "==> Checking Rust..."

if ! command -v cargo >/dev/null 2>&1; then
    echo "==> Rust not found. Installing via rustup..."

    curl --proto '=https' --tlsv1.2 -sSf "$RUSTUP_URL" | sh -s -- -y

    source "$HOME/.cargo/env"
else
    echo "==> Rust already installed"
fi

echo "==> Updating Rust..."
rustup update

echo "==> Installing toolchain components..."
rustup component add rustfmt clippy rust-src llvm-tools-preview

echo "==> Adding OS target..."
rustup target add "$RUST_TARGET"

echo "==> Rust version:"
rustc --version
cargo --version

# =========================
# BINUTILS
# =========================
echo "==> Building binutils $BINUTILS_VER..."

cd build
rm -rf binutils
mkdir binutils
cd binutils

wget -nc "$BINUTILS_URL"
tar -xf "binutils-$BINUTILS_VER.tar.gz"
cd "binutils-$BINUTILS_VER"

mkdir build && cd build

../configure \
    --target=$TARGET \
    --prefix="$PREFIX" \
    --with-sysroot \
    --disable-nls \
    --disable-werror

make -j"$JOBS"
make install

ls -la

cd ../../../..

# =========================
# GCC
# =========================
echo "==> Building gcc $GCC_VER..."

cd build
rm -rf gcc
mkdir gcc
cd gcc

wget -nc "$GCC_URL"
tar -xf "gcc-$GCC_VER.tar.gz"
cd "gcc-$GCC_VER"

# GCC prerequisites
./contrib/download_prerequisites

mkdir build && cd build

../configure \
    --target=$TARGET \
    --prefix="$PREFIX" \
    --disable-nls \
    --enable-languages=c,c++ \
    --without-headers \
    --disable-shared \
    --disable-threads \
    --disable-multilib

make all-gcc -j"$JOBS"
make all-target-libgcc -j"$JOBS"

make install-gcc
make install-target-libgcc

cd ../../../..

# =========================
# GDB
# =========================
echo "==> Building gdb $GDB_VER..."

cd build
rm -rf gdb
mkdir gdb
cd gdb

wget -nc "$GDB_URL"
tar -xf "gdb-$GDB_VER.tar.gz"
cd "gdb-$GDB_VER"

mkdir build && cd build

../configure \
    --target=$TARGET \
    --prefix="$PREFIX" \
    --disable-nls \
    --disable-werror

make -j"$JOBS"
make install

cd ../../../..

echo "==> GDB installed"

rm -rf build

# =========================
# DONE
# =========================
echo ""
echo "=============================="
echo " TOOLCHAIN READY"
echo "=============================="
echo "Add this to your shell:"
echo ""
echo "export PATH=\$HOME/opt/cross/bin:\$PATH"
echo ""
echo "Test:"
echo "  x86_64-elf-gcc --version"
echo ""