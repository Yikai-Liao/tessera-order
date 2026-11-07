# SPDX-License-Identifier: BSD-2-Clause

"""Build pinned native sorts without network access or host-specific tuning."""

from concurrent.futures import ThreadPoolExecutor
import importlib.util
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parent
VENDOR = ROOT.parents[1] / "vendor"
OUT = Path(sys.argv[1]).resolve()
CC = os.environ.get("CC", "cc")
CXX = os.environ.get("CXX", "c++")
SOURCE_SIGNATURE = ""
COMPILERS = {}
BASE = ["-O3", "-DNDEBUG", "-fPIC", "-march=x86-64", "-mtune=generic"]


def run(args: list[str]) -> None:
    """Run one compiler or build tool and propagate its diagnostics."""
    subprocess.run(args, check=True)


def compile_source(task: tuple[Path, str, list[str], Path]) -> Path:
    """Compile one translation unit with the recorded target flags."""
    source, compiler, flags, obj = task
    command = [compiler, *BASE, *flags, "-c", str(source), "-o", str(obj)]
    digest = hashlib.sha256(
        json.dumps(
            [command, SOURCE_SIGNATURE, COMPILERS[compiler]], sort_keys=True
        ).encode()
    )
    digest.update(source.read_bytes())
    digest.update((ROOT / "layout.h").read_bytes())
    signature = digest.hexdigest()
    stamp = obj.with_suffix(".sha256")
    if obj.exists() and stamp.exists() and stamp.read_text() == signature:
        return obj
    run(command)
    stamp.write_text(signature)
    return obj


def main() -> None:
    """Verify fetched sources, then build the benchmark's static libraries."""
    spec = importlib.util.spec_from_file_location("fetch", ROOT.parents[1] / "fetch.py")
    fetch = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(fetch)
    sources = tomllib.loads((ROOT / "sources.toml").read_text())["sources"]
    for name, source in sources.items():
        path = VENDOR / name
        stamp = path / ".tessera-source.json"
        if not stamp.exists():
            raise ValueError(f"{name}: run python3 bench/fetch.py")
        state = json.loads(stamp.read_text())
        if any(state[k] != v for k, v in source.items()) or state[
            "tree_sha256"
        ] != fetch.tree_hash(path):
            raise ValueError(f"{name}: source tree differs; run python3 bench/fetch.py")
    global SOURCE_SIGNATURE
    SOURCE_SIGNATURE = json.dumps(sources, sort_keys=True)
    for compiler in {CC, CXX}:
        COMPILERS[compiler] = subprocess.check_output(
            [compiler, "--version"], text=True
        )
    for variable in ["CFLAGS", "CXXFLAGS", "LDFLAGS"]:
        if os.environ.get(variable):
            raise ValueError(
                f"{variable} must be unset; native comparison uses fixed flags"
            )
    hwy = OUT / "highway"
    run(
        [
            "cmake",
            "-S",
            str(VENDOR / "highway"),
            "-B",
            str(hwy),
            "-DCMAKE_BUILD_TYPE=Release",
            "-DBUILD_SHARED_LIBS=OFF",
            "-DHWY_ENABLE_TESTS=OFF",
            "-DHWY_ENABLE_EXAMPLES=OFF",
            "-DHWY_ENABLE_CONTRIB=OFF",
            "-DHWY_ENABLE_IO=OFF",
            f"-DCMAKE_CXX_COMPILER={CXX}",
            f"-DCMAKE_CXX_FLAGS={' '.join(BASE)}",
        ]
    )
    jobs = min(2, int(os.environ.get("CARGO_BUILD_JOBS", "2")))
    run(["cmake", "--build", str(hwy), "--target", "hwy", "-j", str(jobs)])
    shutil.copyfile(hwy / "libhwy.a", OUT / "libhwy.a")
    includes = [
        "-I" + str(ROOT),
        "-I" + str(VENDOR / "highway"),
        "-I" + str(VENDOR / "x86simdsort/lib"),
    ]
    cpp = ["-std=c++17", *includes]
    tasks = []
    libs: dict[str, list[Path]] = {
        "tessera_native": [],
        "hwy_contrib": [],
        "x86simdsort": [],
    }

    def add(lib: str, source: Path, compiler: str, flags: list[str]) -> None:
        obj = OUT / (lib + "-" + source.stem + ".o")
        tasks.append((source, compiler, flags, obj))
        libs[lib].append(obj)

    for name in ["registry", "simd", "ips"]:
        flags = [*cpp]
        if name == "ips":
            flags += [
                "-U_REENTRANT",
                "-I" + str(VENDOR / "ips2ra/include"),
                "-I" + str(VENDOR / "ips4o/include"),
            ]
        add("tessera_native", ROOT / f"{name}.cpp", CXX, flags)
    for name in ["fluxsort", "crumsort"]:
        add(
            "tessera_native",
            ROOT / f"{name}.c",
            CC,
            ["-std=c11", "-I" + str(VENDOR / name / "src"), "-I" + str(ROOT)],
        )
    add(
        "tessera_native", ROOT / "clausecker.c", CC, ["-I" + str(VENDOR / "clausecker")]
    )
    add("tessera_native", VENDOR / "clausecker/radixsort_permuted.c", CC, [])
    for name in ["vqsort", "vqsort_u32a", "vqsort_u64a"]:
        add("hwy_contrib", VENDOR / f"highway/hwy/contrib/sort/{name}.cc", CXX, cpp)
    for name, arch in [
        ("x86simdsort", "x86-64"),
        ("x86simdsort-avx2", "haswell"),
        ("x86simdsort-skx", "skylake-avx512"),
        ("x86simdsort-icl", "icelake-client"),
        ("x86simdsort-spr", "sapphirerapids"),
    ]:
        add(
            "x86simdsort",
            VENDOR / f"x86simdsort/lib/{name}.cpp",
            CXX,
            [
                *cpp,
                "-I" + str(VENDOR / "x86simdsort/src"),
                "-I" + str(VENDOR / "x86simdsort/utils"),
                "-march=" + arch,
            ],
        )
    tasks.append((ROOT / "memory.cpp", CXX, cpp, OUT / "memory.o"))
    with ThreadPoolExecutor(max_workers=jobs) as pool:
        list(pool.map(compile_source, tasks))
    for name in ["fluxsort", "crumsort"]:
        run(
            [
                "objcopy",
                "--wildcard",
                f"--keep-global-symbol=bench_{name}_*",
                str(OUT / f"tessera_native-{name}.o"),
            ]
        )
    for name, objects in libs.items():
        archive = OUT / f"lib{name}.a"
        archive.unlink(missing_ok=True)
        run([os.environ.get("AR", "ar"), "rcs", str(archive), *map(str, objects)])
    (OUT / "native-build.json").write_text(
        json.dumps(
            {
                "cc": CC,
                "cxx": CXX,
                "flags": BASE,
                "translation_units": [
                    {
                        "source": str(s.relative_to(ROOT.parents[2])),
                        "compiler": c,
                        "flags": BASE + f,
                    }
                    for s, c, f, _ in tasks
                ],
                "ips_parallel": False,
                "libraries": list(libs),
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
