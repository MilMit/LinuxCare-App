#!/usr/bin/env python3
"""Generate a deterministic SPDX 2.3 JSON SBOM from Cargo.lock."""
from __future__ import annotations
import argparse, hashlib, json, os, pathlib, re, subprocess, tomllib
from datetime import datetime, timezone


def created_time(root: pathlib.Path) -> str:
    raw = os.environ.get("SOURCE_DATE_EPOCH")
    if raw is None:
        try:
            raw = subprocess.check_output(
                ["git", "log", "-1", "--format=%ct"], cwd=root, text=True, stderr=subprocess.DEVNULL
            ).strip()
        except (OSError, subprocess.CalledProcessError):
            raw = str(int((root / "Cargo.lock").stat().st_mtime))
    try:
        epoch = max(0, int(raw))
    except ValueError:
        epoch = 0
    return datetime.fromtimestamp(epoch, timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def spdx_id(name: str, version: str, index: int) -> str:
    token = re.sub(r"[^A-Za-z0-9.-]", "-", f"{name}-{version}")
    return f"SPDXRef-Package-{index}-{token}"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--output", default="target/SBOM.spdx.json")
    ap.add_argument("--namespace", default=None)
    args = ap.parse_args()

    root = pathlib.Path(__file__).resolve().parents[1]
    lock_bytes = (root / "Cargo.lock").read_bytes()
    lock = tomllib.loads(lock_bytes.decode())
    manifest = tomllib.loads((root / "Cargo.toml").read_text())
    project = manifest["package"]
    version = project["version"]
    digest = hashlib.sha256(lock_bytes).hexdigest()
    namespace = args.namespace or f"https://milmit.net/linuxcare/spdx/{version}/{digest[:16]}"
    created = created_time(root)

    sorted_pkgs = sorted(lock.get("package", []), key=lambda p: (p["name"], p["version"], p.get("source", "")))
    packages = []
    relationships = []
    root_id = None

    for i, pkg in enumerate(sorted_pkgs, 1):
        pid = spdx_id(pkg["name"], pkg["version"], i)
        is_root = pkg["name"] == project["name"] and pkg["version"] == version and not pkg.get("source")
        if is_root:
            root_id = pid
        item = {
            "SPDXID": pid,
            "name": pkg["name"],
            "versionInfo": pkg["version"],
            "downloadLocation": "NOASSERTION",
            "filesAnalyzed": False,
            "licenseConcluded": "NOASSERTION",
            "licenseDeclared": "NOASSERTION",
            "copyrightText": "NOASSERTION",
        }
        if pkg.get("source"):
            item["externalRefs"] = [{
                "referenceCategory": "PACKAGE-MANAGER",
                "referenceType": "purl",
                "referenceLocator": f"pkg:cargo/{pkg['name']}@{pkg['version']}",
            }]
        if pkg.get("checksum"):
            item["checksums"] = [{"algorithm": "SHA256", "checksumValue": pkg["checksum"]}]
        packages.append(item)

    if root_id is None:
        raise SystemExit("Cargo.lock does not contain the LinuxCare root package")

    for pkg in packages:
        if pkg["SPDXID"] != root_id:
            relationships.append({
                "spdxElementId": root_id,
                "relationshipType": "DEPENDS_ON",
                "relatedSpdxElement": pkg["SPDXID"],
            })

    doc = {
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": f"LinuxCare-{version}-Cargo-SBOM",
        "documentNamespace": namespace,
        "creationInfo": {
            "created": created,
            "creators": ["Tool: LinuxCare scripts/generate-sbom.py"],
        },
        "documentDescribes": [root_id],
        "packages": packages,
        "relationships": relationships,
        "annotations": [{
            "annotationDate": created,
            "annotationType": "OTHER",
            "annotator": "Tool: LinuxCare scripts/generate-sbom.py",
            "comment": f"Generated from Cargo.lock SHA256 {digest}; native OS libraries are outside this Cargo SBOM.",
        }],
    }
    out = root / args.output
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")
    print(out)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
