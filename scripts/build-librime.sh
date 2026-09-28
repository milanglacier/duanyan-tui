#!/usr/bin/env bash
# Builds a self-contained librime.so.1 for the Linux `-bundled` release
# archives. librime ships no Linux binaries, so this compiles it with every
# dependency (boost, glog, leveldb, marisa, opencc, yaml-cpp, libstdc++)
# linked statically and the lua, octagram and predict plugins merged in. The
# result needs only glibc.
#
# The statically linked opencc looks for configs that are in neither rime data
# dir in /usr/share/opencc, like a distribution's librime with its shared
# libopencc. Otherwise it would look in this build's prefix.
#
# Usage: scripts/build-librime.sh <outdir>
#
# Writes <outdir>/librime.so.1, <outdir>/include (the C API headers),
# <outdir>/licenses, <outdir>/version-info.txt and <outdir>/share/opencc (the
# opencc configs and dictionaries, which the archive ships as fallback rime
# shared data). Needs git, curl, cmake, ninja, python3 and a C++17 compiler.
# Run it on the oldest distribution the archive should support (the release
# uses Ubuntu 22.04), since glibc symbol versions come from the build host. To
# try it locally:
#
#   docker run --rm -v "$PWD:/src" -w /src ubuntu:22.04 bash -c \
#     'apt-get update && apt-get install -y build-essential cmake ninja-build \
#        git curl ca-certificates python3 && scripts/build-librime.sh target/librime'
set -euo pipefail

# Keep the versions in sync with the macOS archive of the same librime release
# (its version-info.txt lists the plugin commits).
LIBRIME_VERSION=1.17.0
BOOST_VERSION=1.89.0
BOOST_SHA256=9de758db755e8330a01d995b0a24d09798048400ac25c03fc5ea9be364b13c93
PLUGINS=(
    "hchunhui/librime-lua ec52e48"
    "lotem/librime-octagram dfcc151"
    "rime/librime-predict 920bd41"
)
# Where opencc looks for configs missing from the rime data dirs.
OPENCC_DATA_DIR=/usr/share/opencc
# Newest glibc symbol version the library may require; matches the glibc of
# Ubuntu 22.04, which the release binaries are built on.
MAX_GLIBC=2.35

if [[ $# -ne 1 ]]; then
    echo "usage: $0 <outdir>" >&2
    exit 2
fi
mkdir -p "$1"
out=$(cd "$1" && pwd)
work=${WORK_DIR:-$(mktemp -d)}
mkdir -p "$work"
jobs=$(nproc)
export CMAKE_GENERATOR=Ninja
# The static libraries end up inside a shared object.
export CFLAGS="-O2 -fPIC" CXXFLAGS="-O2 -fPIC"

src=$work/librime
if [[ ! -d $src ]]; then
    git clone --depth 1 --branch "$LIBRIME_VERSION" \
        --recurse-submodules --shallow-submodules \
        https://github.com/rime/librime.git "$src"
fi
# Dependencies install into the source root, where librime's CMake looks.
prefix=$src

# Boost.Regex, the only compiled Boost component librime needs on Linux.
boost=$work/boost-$BOOST_VERSION
if [[ ! -f $boost/stage/lib/libboost_regex.a ]]; then
    tarball=$work/boost_${BOOST_VERSION//./_}.tar.gz
    curl -fsSL -o "$tarball" \
        "https://archives.boost.io/release/$BOOST_VERSION/source/$(basename "$tarball")"
    echo "$BOOST_SHA256  $tarball" | sha256sum -c
    tar -xzf "$tarball" -C "$work"
    mv "$work/boost_${BOOST_VERSION//./_}" "$boost"
    (cd "$boost" && ./bootstrap.sh --with-libraries=regex &&
        ./b2 -q -j"$jobs" link=static variant=release cxxflags=-fPIC stage)
fi

cmake_dep() {
    local dir=$1
    shift
    cmake -S "$src/deps/$dir" -B "$work/build-$dir" \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_INSTALL_PREFIX="$prefix" \
        -DCMAKE_POSITION_INDEPENDENT_CODE=ON \
        -DBUILD_SHARED_LIBS=OFF \
        -DBUILD_TESTING=OFF \
        "$@"
    cmake --build "$work/build-$dir" --target install
}

# Optional system libraries are disabled so the result does not depend on
# whatever the build host happens to have installed.
cmake_dep glog -DWITH_GFLAGS=OFF -DWITH_GTEST=OFF -DWITH_UNWIND=none
cmake_dep leveldb -DLEVELDB_BUILD_TESTS=OFF -DLEVELDB_BUILD_BENCHMARKS=OFF \
    -DHAVE_CRC32C=OFF -DHAVE_SNAPPY=OFF -DHAVE_TCMALLOC=OFF
cmake_dep marisa-trie -DENABLE_TOOLS=OFF
# The quotes survive the shell that runs the compiler: PKGDATADIR is a string.
cmake_dep opencc \
    "-DCMAKE_CXX_FLAGS=$CXXFLAGS -UPKGDATADIR -DPKGDATADIR='\"$OPENCC_DATA_DIR\"'"
cmake_dep yaml-cpp -DYAML_CPP_BUILD_CONTRIB=OFF -DYAML_CPP_BUILD_TESTS=OFF \
    -DYAML_CPP_BUILD_TOOLS=OFF

echo "librime $LIBRIME_VERSION" >"$work/version-info.txt"
for plugin in "${PLUGINS[@]}"; do
    read -r slug rev <<<"$plugin"
    name=${slug##*/}
    dir=$src/plugins/${name#librime-}
    if [[ ! -d $dir ]]; then
        git clone "https://github.com/$slug.git" "$dir"
        git -C "$dir" checkout "$rev"
        # librime-lua fetches its in-tree lua sources here.
        if [[ -f $dir/action-install.sh ]]; then
            (cd "$dir" && bash ./action-install.sh)
        fi
    fi
    echo "$slug $rev" >>"$work/version-info.txt"
done

cmake -S "$src" -B "$work/build-librime" \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX="$work/dist" \
    -DBUILD_STATIC=ON \
    -DBUILD_MERGED_PLUGINS=ON \
    -DENABLE_EXTERNAL_PLUGINS=OFF \
    -DBUILD_TEST=OFF \
    -DCMAKE_DISABLE_FIND_PACKAGE_Gflags=ON \
    -DBoost_NO_BOOST_CMAKE=ON \
    -DBOOST_ROOT="$boost" \
    -DCMAKE_SHARED_LINKER_FLAGS="-static-libstdc++ -static-libgcc -Wl,--exclude-libs,ALL"
cmake --build "$work/build-librime" --target install

rm -rf "$out/include" "$out/licenses" "$out/share"
cp -L "$work/dist/lib/librime.so.1" "$out/librime.so.1"
strip --strip-unneeded "$out/librime.so.1"
cp -r "$work/dist/include" "$out/include"
mkdir "$out/share"
cp -r "$prefix/share/opencc" "$out/share/opencc"
cp "$work/version-info.txt" "$out/version-info.txt"

mkdir "$out/licenses"
cp "$src/LICENSE" "$out/licenses/librime"
cp "$boost/LICENSE_1_0.txt" "$out/licenses/boost"
cp "$src/deps/glog/COPYING" "$out/licenses/glog"
cp "$src/deps/leveldb/LICENSE" "$out/licenses/leveldb"
cp "$src/deps/marisa-trie/COPYING.md" "$out/licenses/marisa-trie"
cp "$src/deps/opencc/LICENSE" "$out/licenses/opencc"
cp "$src/deps/yaml-cpp/LICENSE" "$out/licenses/yaml-cpp"
for plugin in "${PLUGINS[@]}"; do
    read -r slug _ <<<"$plugin"
    name=${slug##*/}
    cp "$src/plugins/${name#librime-}/LICENSE" "$out/licenses/$name"
done
# librime-lua compiles in lua, whose license is only at the end of lua.h.
sed -n '/^\* Copyright (C) 1994/,/^\*\*\*/{/^\*\*\*/d;s/^\* \{0,1\}//;p;}' \
    "$src/plugins/lua/thirdparty/lua5.4/lua.h" >"$out/licenses/lua"
[[ -s $out/licenses/lua ]]

# Refuse to publish a library that would not load on the target systems.
lib=$out/librime.so.1
mapfile -t needed < <(readelf -d "$lib" | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p')
echo "NEEDED: ${needed[*]}"
for so in "${needed[@]}"; do
    case $so in
    libc.so.* | libm.so.* | libpthread.so.* | libdl.so.* | librt.so.* | ld-linux*) ;;
    *)
        echo "error: $lib depends on $so" >&2
        exit 1
        ;;
    esac
done
# The build prefix must not leak into opencc's data lookup.
if ! grep -aqF "$OPENCC_DATA_DIR/" "$lib" || grep -aqF "$prefix/share/opencc" "$lib"; then
    echo "error: opencc in $lib does not look for data in $OPENCC_DATA_DIR" >&2
    exit 1
fi
glibc=$(objdump -T "$lib" | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -uV | tail -n1)
echo "newest GLIBC symbol: $glibc"
if [[ $(printf '%s\n' "$glibc" "$MAX_GLIBC" | sort -V | tail -n1) != "$MAX_GLIBC" ]]; then
    echo "error: $lib needs GLIBC_$glibc, newer than $MAX_GLIBC" >&2
    exit 1
fi
