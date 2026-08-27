#!/usr/bin/env python3
"""
PVG 0.1 - Native Android Cross-Compilation Script
Compiles native Rust binaries for both arm64-v8a (Physical Hardware) and x86_64 (Android Studio Emulator)
"""

import argparse
import glob
import os
import shutil
import subprocess
import sys
import time

GREEN = "\033[92m"
CYAN = "\033[96m"
YELLOW = "\033[93m"
RED = "\033[91m"
BOLD = "\033[1m"
RESET = "\033[0m"

TARGET_MAP = {
    "arm64-v8a": "aarch64-linux-android",
    "x86_64": "x86_64-linux-android",
}


def log_info(msg: str):
    print(f"{CYAN}{BOLD}[INFO]{RESET} {msg}")


def log_success(msg: str):
    print(f"{GREEN}{BOLD}[SUCCESS]{RESET} {msg}")


def log_warn(msg: str):
    print(f"{YELLOW}{BOLD}[WARN]{RESET} {msg}")


def log_error(msg: str):
    print(f"{RED}{BOLD}[ERROR]{RESET} {msg}")


def find_android_ndk() -> str | None:
    env_vars = ["ANDROID_NDK_HOME", "ANDROID_NDK_ROOT", "NDK_HOME", "ANDROID_NDK"]
    for var in env_vars:
        path = os.environ.get(var)
        if path and os.path.isdir(path):
            return path

    sdk_roots = []
    if os.name == "nt":
        local_app_data = os.environ.get("LOCALAPPDATA", "")
        if local_app_data:
            sdk_roots.append(os.path.join(local_app_data, "Android", "Sdk"))
        sdk_roots.extend([
            r"C:\Android\Sdk",
            r"C:\Android\sdk",
            r"C:\Program Files (x86)\Android\android-sdk",
        ])
    else:
        home = os.path.expanduser("~")
        sdk_roots.extend([
            os.path.join(home, "Android", "Sdk"),
            os.path.join(home, "Library", "Android", "sdk"),
            "/opt/android-sdk",
        ])

    for sdk in sdk_roots:
        ndk_base = os.path.join(sdk, "ndk")
        if os.path.isdir(ndk_base):
            versions = sorted(glob.glob(os.path.join(ndk_base, "*")), reverse=True)
            for ver_dir in versions:
                if os.path.isdir(ver_dir):
                    return ver_dir

    return None


def ensure_cargo_ndk():
    if shutil.which("cargo-ndk") is None:
        log_warn("'cargo-ndk' was not found in PATH. Installing via 'cargo install cargo-ndk'...")
        res = subprocess.run(["cargo", "install", "cargo-ndk"], shell=(os.name == "nt"))
        if res.returncode != 0:
            log_error("Failed to install 'cargo-ndk'.")
            sys.exit(1)
        log_success("'cargo-ndk' installed successfully.")


def ensure_rust_target(rust_target: str):
    res = subprocess.run(
        ["rustup", "target", "add", rust_target],
        capture_output=True,
        text=True,
        shell=(os.name == "nt"),
    )
    if res.returncode != 0:
        log_warn(f"rustup target add {rust_target} output: {res.stderr.strip()}")


def main():
    parser = argparse.ArgumentParser(description="Cross-compile PVG Native Android Engine (.so)")
    parser.add_argument(
        "--abi",
        choices=["all", "arm64-v8a", "x86_64"],
        default="all",
        help="Target ABI to compile (default: all)",
    )
    parser.add_argument(
        "--debug",
        action="store_true",
        help="Build in debug mode instead of --release",
    )
    parser.add_argument(
        "--clean",
        action="store_true",
        help="Clean the output jniLibs folder before building",
    )
    args = parser.parse_args()

    project_root = os.path.dirname(os.path.abspath(__file__))
    output_jni_dir = os.path.join(project_root, "android", "app", "src", "main", "jniLibs")

    print(f"\n{BOLD}==============================================================={RESET}")
    print(f"{CYAN}{BOLD}   ⚡ PVG 0.1 Native Android Engine Cross-Compiler for Windows   {RESET}")
    print(f"{BOLD}==============================================================={RESET}\n")

    ndk_path = find_android_ndk()
    if not ndk_path:
        log_error(
            "Android NDK not found! Set the ANDROID_NDK_HOME environment variable or "
            "install the NDK via Android Studio SDK Manager."
        )
        sys.exit(1)

    os.environ["ANDROID_NDK_HOME"] = ndk_path
    log_info(f"Using Android NDK: {ndk_path}")

    if shutil.which("cargo") is None or shutil.which("rustup") is None:
        log_error("Rust toolchain (cargo / rustup) not found in system PATH.")
        sys.exit(1)

    ensure_cargo_ndk()

    if args.clean and os.path.exists(output_jni_dir):
        log_info(f"Cleaning existing jniLibs directory: {output_jni_dir}")
        shutil.rmtree(output_jni_dir)

    os.makedirs(output_jni_dir, exist_ok=True)

    if args.abi == "all":
        abis_to_build = list(TARGET_MAP.keys())
    else:
        abis_to_build = [args.abi]

    build_mode_flag = [] if args.debug else ["--release"]
    mode_str = "debug" if args.debug else "release"

    log_info(f"Building targets: {', '.join(abis_to_build)} in [{mode_str}] mode...")
    start_time = time.time()
    built_files = []

    for abi in abis_to_build:
        rust_target = TARGET_MAP[abi]
        log_info(f"──> Building ABI [{abi}] (Rust Target: {rust_target})...")

        ensure_rust_target(rust_target)

        cmd = [
            "cargo",
            "ndk",
            "-t",
            abi,
            "-o",
            output_jni_dir,
            "build",
        ] + build_mode_flag + [
            "-p",
            "pvg_android",
        ]

        t0 = time.time()
        res = subprocess.run(cmd, cwd=project_root, shell=(os.name == "nt"))
        elapsed = time.time() - t0

        if res.returncode != 0:
            log_error(f"Failed to build ABI: {abi}")
            sys.exit(res.returncode)

        so_path = os.path.join(output_jni_dir, abi, "libpvg_android.so")
        if os.path.exists(so_path):
            size_kb = os.path.getsize(so_path) / 1024.0
            built_files.append((abi, so_path, size_kb, elapsed))
            log_success(f"Built {abi}/libpvg_android.so ({size_kb:.1f} KB in {elapsed:.2f}s)")
        else:
            log_warn(f"Compiled successfully but {so_path} was not found at expected location.")

    total_elapsed = time.time() - start_time

    print(f"\n{BOLD}┌─────────────────────────────────────────────────────────────┐{RESET}")
    print(f"{BOLD}│                  BUILD SUMMARY RESULTS                      │{RESET}")
    print(f"{BOLD}├───────────────┬──────────────────────┬───────────┬──────────┤{RESET}")
    print(f"{BOLD}│ Android ABI   │ Binary Name          │ Size (KB) │ Time (s) │{RESET}")
    print(f"{BOLD}├───────────────┼──────────────────────┼───────────┼──────────┤{RESET}")
    for abi, _, size_kb, dur in built_files:
        print(f"│ {abi:<13} │ libpvg_android.so    │ {size_kb:>8.1f}  │ {dur:>7.2f}s │")
    print(f"{BOLD}└───────────────┴──────────────────────┴───────────┴──────────┘{RESET}")
    log_success(f"All {len(built_files)} ABI(s) compiled successfully in {total_elapsed:.2f}s!")
    log_info(f"Binaries located at: {output_jni_dir}\n")


if __name__ == "__main__":
    main()