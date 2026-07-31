# MCX default theme
# Green accent, package-aware prompt showing the active package.

ACCENT = "#22c55e"
CWD_COLOR = "#a78bfa"
DIM = "#6b7280"
SUCCESS = "#22c55e"
ERROR = "#ef4444"


def render_prompt(**context):
    cwd = context.get("cwd", "~")
    exit_code = int(context.get("exit_code", "0") or "0")
    pkg = context.get("package", "")
    status = "\u2714" if exit_code == 0 else "\u2718"

    lines_above = []
    if pkg:
        lines_above.append(f" [{pkg}]")

    return {
        "lines_above": lines_above,
        "input_prefix": f"{cwd} \u276f ",
        "right_prompt": status,
        "colors": {
            "accent": ACCENT,
            "cwd": CWD_COLOR,
            "success": SUCCESS,
            "error": ERROR,
            "dim": DIM,
        },
    }


def render_right_prompt(**context):
    pkg = context.get("package", "")
    if not pkg:
        return ""
    return pkg


def render_command_summary(**context):
    cmd = context.get("command", "")
    exit_code = int(context.get("exit_code", "0") or "0")
    status = "\u2714" if exit_code == 0 else "\u2718"
    return f"[{status}] {cmd} (exit {exit_code})"