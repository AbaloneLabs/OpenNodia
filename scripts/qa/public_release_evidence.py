#!/usr/bin/env python3
"""Create non-secret public-release evidence fragments from real validation runs."""

from __future__ import annotations

import argparse
import json
import os
import platform
import re
import subprocess
import tempfile
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[2]
UTC_PATTERN = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$")
HTTPS_PATTERN = re.compile(r"^https://[^\s]+$")
TAG_PATTERN = re.compile(r"^v[0-9A-Za-z._-]+$")
DIGEST_PATTERN = re.compile(r"^sha256:[0-9a-f]{64}$")
COMMIT_PATTERN = re.compile(r"^[0-9a-f]{40}$")
ROOT_KEYS = {"cross_platform", "release_artifact", "external_review", "upgrade_rollback"}
CROSS_PLATFORM_KEYS = {"macos", "windows", "linux_arm64"}
DESKTOP_KEYS = {"install_upgrade_reboot", "secret_reuse", "run_at_utc", "os", "architecture", "run_url"}
LINUX_ARM64_KEYS = {
    "full_stack_sync_restart",
    "run_at_utc",
    "os",
    "architecture",
    "docker_version",
    "compose_version",
    "run_url",
}
RELEASE_ARTIFACT_KEYS = {
    "tag",
    "workflow_run_url",
    "git_commit",
    "image_digest",
    "checksums_verified",
    "provenance_verified",
    "signatures_verified",
    "licenses_verified",
}
EXTERNAL_REVIEW_KEYS = {"status", "reviewer", "completed_at_utc", "report_url", "blockers_open"}
UPGRADE_ROLLBACK_KEYS = {
    "clean_install",
    "upgrade",
    "rollback",
    "testnet_validation",
    "run_at_utc",
    "source_version",
    "target_version",
    "run_url",
}


def utc_now() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def require_https_url(value: str, field: str) -> str:
    if not HTTPS_PATTERN.match(value):
        raise SystemExit(f"{field} must be an https URL")
    return value


def run(cmd: list[str], *, env: dict[str, str] | None = None, cwd: Path = REPO_ROOT) -> str:
    proc = subprocess.run(
        cmd,
        cwd=cwd,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if proc.returncode != 0:
        message = proc.stderr.strip() or proc.stdout.strip()
        raise SystemExit(f"command failed ({' '.join(cmd)}): {message}")
    return proc.stdout.strip()


def read_json_url(url: str) -> dict[str, Any]:
    with urllib.request.urlopen(url, timeout=10) as response:
        return json.loads(response.read().decode("utf-8"))


def write_json(data: dict[str, Any], output: Path | None) -> None:
    text = json.dumps(data, indent=2, sort_keys=True)
    if output:
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(text + "\n", encoding="utf-8")
    else:
        print(text)


def read_json_file(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        raise SystemExit(f"{path} is not valid JSON: {error}") from error


def read_json_object(path: Path, label: str) -> dict[str, Any]:
    data = read_json_file(path)
    if not isinstance(data, dict):
        raise SystemExit(f"{label} must be a JSON object: {path}")
    return data


def deep_merge(base: dict[str, Any], incoming: dict[str, Any]) -> dict[str, Any]:
    merged = dict(base)
    for key, value in incoming.items():
        if isinstance(value, dict) and isinstance(merged.get(key), dict):
            merged[key] = deep_merge(merged[key], value)
        else:
            merged[key] = value
    return merged


def reject_unexpected_keys(item: dict[str, Any], allowed: set[str], errors: list[str], label: str) -> None:
    unexpected = set(item) - allowed
    if unexpected:
        errors.append(f"{label} has unexpected keys: {', '.join(sorted(unexpected))}")


def detect_schema_errors(data: Any) -> list[str]:
    errors: list[str] = []
    if not isinstance(data, dict):
        return ["public release evidence must be an object"]
    reject_unexpected_keys(data, ROOT_KEYS, errors, "public release evidence")

    cross = data.get("cross_platform")
    if isinstance(cross, dict):
        reject_unexpected_keys(cross, CROSS_PLATFORM_KEYS, errors, "cross_platform")
        for name in ("macos", "windows"):
            item = cross.get(name)
            if item is None:
                continue
            errors.extend(validate_desktop_evidence(name, item))
        arm = cross.get("linux_arm64")
        if arm is not None:
            errors.extend(validate_linux_arm64_evidence(arm))
    elif cross is not None:
        errors.append("cross_platform must be an object")

    release = data.get("release_artifact")
    if release is not None:
        errors.extend(validate_release_artifact_evidence(release))

    review = data.get("external_review")
    if review is not None:
        errors.extend(validate_external_review_evidence(review))

    upgrade = data.get("upgrade_rollback")
    if upgrade is not None:
        errors.extend(validate_upgrade_rollback_evidence(upgrade))

    return errors


def detect_completeness_errors(data: Any) -> list[str]:
    errors: list[str] = []
    if not isinstance(data, dict):
        return []
    required_root = {"cross_platform", "release_artifact", "external_review", "upgrade_rollback"}
    missing_root = required_root - set(data)
    if missing_root:
        errors.append(f"missing top-level keys: {', '.join(sorted(missing_root))}")
    cross = data.get("cross_platform")
    if not isinstance(cross, dict):
        errors.append("cross_platform must be an object")
        return errors
    missing_platforms = {"macos", "windows", "linux_arm64"} - set(cross)
    if missing_platforms:
        errors.append(f"missing cross_platform keys: {', '.join(sorted(missing_platforms))}")
    return errors


def require_value(item: dict[str, Any], key: str, expected: Any, errors: list[str], label: str) -> None:
    if item.get(key) != expected:
        errors.append(f"{label}.{key} must be {expected!r}")


def require_non_empty_string(item: dict[str, Any], key: str, errors: list[str], label: str) -> None:
    if not isinstance(item.get(key), str) or not item[key]:
        errors.append(f"{label}.{key} must be a non-empty string")


def require_utc(item: dict[str, Any], key: str, errors: list[str], label: str) -> None:
    if not isinstance(item.get(key), str) or not UTC_PATTERN.match(item[key]):
        errors.append(f"{label}.{key} must be a UTC timestamp")


def require_https(item: dict[str, Any], key: str, errors: list[str], label: str) -> None:
    if not isinstance(item.get(key), str) or not HTTPS_PATTERN.match(item[key]):
        errors.append(f"{label}.{key} must be an https URL")


def validate_desktop_evidence(name: str, item: Any) -> list[str]:
    errors: list[str] = []
    label = f"cross_platform.{name}"
    if not isinstance(item, dict):
        return [f"{label} must be an object"]
    reject_unexpected_keys(item, DESKTOP_KEYS, errors, label)
    require_value(item, "install_upgrade_reboot", "passed", errors, label)
    require_value(item, "secret_reuse", "passed", errors, label)
    require_utc(item, "run_at_utc", errors, label)
    require_non_empty_string(item, "os", errors, label)
    require_non_empty_string(item, "architecture", errors, label)
    require_https(item, "run_url", errors, label)
    return errors


def validate_linux_arm64_evidence(item: Any) -> list[str]:
    errors: list[str] = []
    label = "cross_platform.linux_arm64"
    if not isinstance(item, dict):
        return [f"{label} must be an object"]
    reject_unexpected_keys(item, LINUX_ARM64_KEYS, errors, label)
    require_value(item, "full_stack_sync_restart", "passed", errors, label)
    require_value(item, "architecture", "arm64", errors, label)
    require_utc(item, "run_at_utc", errors, label)
    require_non_empty_string(item, "os", errors, label)
    require_non_empty_string(item, "docker_version", errors, label)
    require_non_empty_string(item, "compose_version", errors, label)
    require_https(item, "run_url", errors, label)
    return errors


def validate_restart_status(status_before: Any, status_after: Any) -> None:
    if not isinstance(status_before, dict):
        raise SystemExit("OpenNodia API status before restart is not a JSON object")
    if not isinstance(status_after, dict):
        raise SystemExit("OpenNodia API status after restart is not a JSON object")
    if status_before.get("node_reachable") is not True:
        raise SystemExit("OpenNodia API status before restart does not report a reachable node")
    if status_after.get("node_reachable") is not True:
        raise SystemExit("OpenNodia API status after restart does not report a reachable node")
    for key in ("setup_complete", "network"):
        if status_before.get(key) != status_after.get(key):
            raise SystemExit(
                "OpenNodia API status changed after restart: "
                f"{key} before={status_before.get(key)!r} after={status_after.get(key)!r}"
            )


def validate_release_artifact_evidence(item: Any) -> list[str]:
    errors: list[str] = []
    label = "release_artifact"
    if not isinstance(item, dict):
        return [f"{label} must be an object"]
    reject_unexpected_keys(item, RELEASE_ARTIFACT_KEYS, errors, label)
    if not isinstance(item.get("tag"), str) or not TAG_PATTERN.match(item["tag"]):
        errors.append("release_artifact.tag must start with v")
    require_https(item, "workflow_run_url", errors, label)
    if not isinstance(item.get("git_commit"), str) or not COMMIT_PATTERN.match(item["git_commit"]):
        errors.append("release_artifact.git_commit must be a full commit hash")
    if not isinstance(item.get("image_digest"), str) or not DIGEST_PATTERN.match(item["image_digest"]):
        errors.append("release_artifact.image_digest must be sha256:<64 hex>")
    for key in ("checksums_verified", "provenance_verified", "signatures_verified", "licenses_verified"):
        require_value(item, key, True, errors, label)
    return errors


def validate_external_review_evidence(item: Any) -> list[str]:
    errors: list[str] = []
    label = "external_review"
    if not isinstance(item, dict):
        return [f"{label} must be an object"]
    reject_unexpected_keys(item, EXTERNAL_REVIEW_KEYS, errors, label)
    require_value(item, "status", "passed", errors, label)
    require_non_empty_string(item, "reviewer", errors, label)
    require_utc(item, "completed_at_utc", errors, label)
    require_https(item, "report_url", errors, label)
    require_value(item, "blockers_open", 0, errors, label)
    return errors


def validate_upgrade_rollback_evidence(item: Any) -> list[str]:
    errors: list[str] = []
    label = "upgrade_rollback"
    if not isinstance(item, dict):
        return [f"{label} must be an object"]
    reject_unexpected_keys(item, UPGRADE_ROLLBACK_KEYS, errors, label)
    for key in ("clean_install", "upgrade", "rollback", "testnet_validation"):
        require_value(item, key, "passed", errors, label)
    require_utc(item, "run_at_utc", errors, label)
    require_non_empty_string(item, "source_version", errors, label)
    require_non_empty_string(item, "target_version", errors, label)
    require_https(item, "run_url", errors, label)
    return errors


def command_desktop(args: argparse.Namespace) -> None:
    require_https_url(args.run_url, "--run-url")
    expected_system = {"macos": "Darwin", "windows": "Windows"}[args.platform]
    actual_system = platform.system()
    if actual_system != expected_system:
        raise SystemExit(f"--platform {args.platform} must be recorded on {expected_system}, got {actual_system}")
    if not args.confirm_install_upgrade_reboot:
        raise SystemExit("pass --confirm-install-upgrade-reboot only after real install, upgrade, and reboot validation")

    with tempfile.TemporaryDirectory(prefix="opennodia-desktop-evidence-") as tmp:
        tmp_path = Path(tmp)
        env_file = tmp_path / "opennodia.env"
        secrets_dir = tmp_path / "secrets"
        env = os.environ.copy()
        env["OPENNODIA_SECRETS_DIR"] = str(secrets_dir)
        env["OPENNODIA_BIND_ADDRESS"] = "127.0.0.1"
        if args.platform == "windows":
            script = REPO_ROOT / "scripts" / "init-secrets.ps1"
            run(["pwsh", "-NoProfile", "-File", str(script), "-EnvironmentFile", str(env_file)], env=env)
            first_algod = (secrets_dir / "algod.token").read_bytes()
            first_db = (secrets_dir / "indexer-db-password").read_bytes()
            run(["pwsh", "-NoProfile", "-File", str(script), "-EnvironmentFile", str(env_file)], env=env)
        else:
            env["OPENNODIA_ENV_FILE"] = str(env_file)
            script = REPO_ROOT / "scripts" / "init-secrets.sh"
            run([str(script)], env=env)
            first_algod = (secrets_dir / "algod.token").read_bytes()
            first_db = (secrets_dir / "indexer-db-password").read_bytes()
            run([str(script)], env=env)
        if first_algod != (secrets_dir / "algod.token").read_bytes():
            raise SystemExit("algod token changed after init-secrets rerun")
        if first_db != (secrets_dir / "indexer-db-password").read_bytes():
            raise SystemExit("indexer DB password changed after init-secrets rerun")

    fragment = {
        "cross_platform": {
            args.platform: {
                "install_upgrade_reboot": "passed",
                "secret_reuse": "passed",
                "run_at_utc": utc_now(),
                "os": platform.platform(),
                "architecture": platform.machine(),
                "run_url": args.run_url,
            }
        }
    }
    write_json(fragment, args.output)


def command_linux_arm64(args: argparse.Namespace) -> None:
    require_https_url(args.run_url, "--run-url")
    machine = platform.machine().lower()
    if machine not in {"aarch64", "arm64"}:
        raise SystemExit(f"linux_arm64 evidence must run on ARM64, got {platform.machine()}")
    if platform.system() != "Linux":
        raise SystemExit(f"linux_arm64 evidence must run on Linux, got {platform.system()}")
    if not args.confirm_full_stack_sync_restart:
        raise SystemExit("pass --confirm-full-stack-sync-restart only after real sync and restart validation")

    docker_version = run(["docker", "--version"])
    compose_version = run(["docker", "compose", "version"])
    status_before = read_json_url(args.api_url.rstrip("/") + "/api/status")
    run(["docker", "compose", "ps"], cwd=args.compose_dir)
    run(["docker", "compose", "restart", args.compose_service], cwd=args.compose_dir)
    status_after = read_json_url(args.api_url.rstrip("/") + "/api/status")
    validate_restart_status(status_before, status_after)

    fragment = {
        "cross_platform": {
            "linux_arm64": {
                "full_stack_sync_restart": "passed",
                "run_at_utc": utc_now(),
                "os": platform.platform(),
                "architecture": "arm64",
                "docker_version": docker_version,
                "compose_version": compose_version,
                "run_url": args.run_url,
            }
        }
    }
    write_json(fragment, args.output)


def command_release_artifact(args: argparse.Namespace) -> None:
    require_https_url(args.workflow_run_url, "--workflow-run-url")
    if not TAG_PATTERN.match(args.tag):
        raise SystemExit("--tag must match v*")
    if not DIGEST_PATTERN.match(args.image_digest):
        raise SystemExit("--image-digest must be sha256:<64 hex>")
    git_commit = args.git_commit or run(["git", "rev-parse", "HEAD"])
    if not COMMIT_PATTERN.match(git_commit):
        raise SystemExit("--git-commit must be a full 40-character commit hash")

    env = os.environ.copy()
    if args.require_signatures:
        env["OPENNODIA_REQUIRE_SIGNATURES"] = "true"
    if args.certificate_identity:
        env["OPENNODIA_COSIGN_CERTIFICATE_IDENTITY"] = args.certificate_identity
    if args.certificate_oidc_issuer:
        env["OPENNODIA_COSIGN_CERTIFICATE_OIDC_ISSUER"] = args.certificate_oidc_issuer
    if args.image_ref:
        env["OPENNODIA_IMAGE_REF"] = args.image_ref
    run([str(REPO_ROOT / "scripts" / "release" / "verify-release.sh"), str(args.release_dir)], env=env)
    licenses_dir = args.release_dir / "licenses"
    if not licenses_dir.exists() or not any(licenses_dir.iterdir()):
        raise SystemExit(f"release licenses directory is missing or empty: {licenses_dir}")

    fragment = {
        "release_artifact": {
            "tag": args.tag,
            "workflow_run_url": args.workflow_run_url,
            "git_commit": git_commit,
            "image_digest": args.image_digest,
            "checksums_verified": True,
            "provenance_verified": True,
            "signatures_verified": bool(args.require_signatures),
            "licenses_verified": True,
        }
    }
    errors = validate_release_artifact_evidence(fragment["release_artifact"])
    if errors:
        raise SystemExit("\n".join(errors))
    write_json(fragment, args.output)


def command_external_review(args: argparse.Namespace) -> None:
    require_https_url(args.report_url, "--report-url")
    if args.blockers_open != 0:
        raise SystemExit("public release evidence requires zero open blockers")
    completed_at = args.completed_at_utc or utc_now()
    if not UTC_PATTERN.match(completed_at):
        raise SystemExit("--completed-at-utc must be YYYY-MM-DDTHH:MM:SSZ")
    fragment = {
        "external_review": {
            "status": "passed",
            "reviewer": args.reviewer,
            "completed_at_utc": completed_at,
            "report_url": args.report_url,
            "blockers_open": args.blockers_open,
        }
    }
    write_json(fragment, args.output)


def command_upgrade_rollback(args: argparse.Namespace) -> None:
    require_https_url(args.run_url, "--run-url")
    if not args.confirm_clean_install_upgrade_rollback:
        raise SystemExit("pass --confirm-clean-install-upgrade-rollback only after the real validation run")
    fragment = {
        "upgrade_rollback": {
            "clean_install": "passed",
            "upgrade": "passed",
            "rollback": "passed",
            "testnet_validation": "passed",
            "run_at_utc": utc_now(),
            "source_version": args.source_version,
            "target_version": args.target_version,
            "run_url": args.run_url,
        }
    }
    write_json(fragment, args.output)


def command_merge(args: argparse.Namespace) -> None:
    merged: dict[str, Any] = {}
    if args.base and args.base.exists():
        merged = read_json_object(args.base, "base evidence")
    for fragment_path in args.fragments:
        fragment = read_json_object(fragment_path, "evidence fragment")
        merged = deep_merge(merged, fragment)
    errors = detect_schema_errors(merged)
    if args.complete:
        errors.extend(detect_completeness_errors(merged))
    if errors:
        raise SystemExit("\n".join(errors))
    write_json(merged, args.output)


def command_validate(args: argparse.Namespace) -> None:
    data = read_json_file(args.file)
    errors = detect_schema_errors(data)
    if args.complete:
        errors.extend(detect_completeness_errors(data))
    if errors:
        raise SystemExit("\n".join(errors))
    print(f"Public release evidence shape is valid: {args.file}")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    desktop = sub.add_parser("desktop", help="Record macOS or Windows install evidence")
    desktop.add_argument("--platform", choices=["macos", "windows"], required=True)
    desktop.add_argument("--run-url", required=True)
    desktop.add_argument("--confirm-install-upgrade-reboot", action="store_true")
    desktop.add_argument("--output", type=Path)
    desktop.set_defaults(func=command_desktop)

    arm = sub.add_parser("linux-arm64", help="Record Linux ARM64 full-stack evidence")
    arm.add_argument("--run-url", required=True)
    arm.add_argument("--api-url", default="http://127.0.0.1:30080")
    arm.add_argument("--compose-dir", type=Path, default=REPO_ROOT)
    arm.add_argument("--compose-service", default="opennodia")
    arm.add_argument("--confirm-full-stack-sync-restart", action="store_true")
    arm.add_argument("--output", type=Path)
    arm.set_defaults(func=command_linux_arm64)

    release = sub.add_parser("release-artifact", help="Record signed release artifact evidence")
    release.add_argument("--tag", required=True)
    release.add_argument("--workflow-run-url", required=True)
    release.add_argument("--image-digest", required=True)
    release.add_argument("--release-dir", type=Path, default=REPO_ROOT / "dist" / "release")
    release.add_argument("--git-commit")
    release.add_argument("--require-signatures", action="store_true")
    release.add_argument("--certificate-identity")
    release.add_argument("--certificate-oidc-issuer")
    release.add_argument("--image-ref")
    release.add_argument("--output", type=Path)
    release.set_defaults(func=command_release_artifact)

    review = sub.add_parser("external-review", help="Record completed external review evidence")
    review.add_argument("--reviewer", required=True)
    review.add_argument("--report-url", required=True)
    review.add_argument("--completed-at-utc")
    review.add_argument("--blockers-open", type=int, default=0)
    review.add_argument("--output", type=Path)
    review.set_defaults(func=command_external_review)

    upgrade = sub.add_parser("upgrade-rollback", help="Record upgrade and rollback evidence")
    upgrade.add_argument("--source-version", required=True)
    upgrade.add_argument("--target-version", required=True)
    upgrade.add_argument("--run-url", required=True)
    upgrade.add_argument("--confirm-clean-install-upgrade-rollback", action="store_true")
    upgrade.add_argument("--output", type=Path)
    upgrade.set_defaults(func=command_upgrade_rollback)

    merge = sub.add_parser("merge", help="Merge evidence fragments")
    merge.add_argument("--base", type=Path)
    merge.add_argument("--output", type=Path, required=True)
    merge.add_argument("--complete", action="store_true")
    merge.add_argument("fragments", type=Path, nargs="+")
    merge.set_defaults(func=command_merge)

    validate = sub.add_parser("validate", help="Validate evidence shape without jq")
    validate.add_argument("file", type=Path)
    validate.add_argument("--complete", action="store_true")
    validate.set_defaults(func=command_validate)

    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()
    args.func(args)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
