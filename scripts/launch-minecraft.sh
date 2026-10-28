#!/bin/bash
# Optional standalone helper. The TUI launches Prism directly and does not need
# this script installed. Prism expects the instance directory ID, not its label.
# Usage: launch-minecraft.sh "instance-id" [--hide-launcher]

set -euo pipefail

if [ -z "${1:-}" ] || [ "$#" -gt 2 ]; then
    echo "Usage: $0 <instance-id> [--hide-launcher]" >&2
    exit 1
fi

instance_id=$1
qt_args=()
if [ "${2:-}" = "--hide-launcher" ]; then
    qt_args=(-platform offscreen)
elif [ "$#" -eq 2 ]; then
    echo "Unknown option: $2" >&2
    exit 1
fi

if command -v prismlauncher >/dev/null 2>&1; then
    launcher=(prismlauncher)
else
    launcher=(flatpak run org.prismlauncher.PrismLauncher)
fi

umask 077
launch_state_dir=${XDG_STATE_HOME:-$HOME/.local/state}/minecraft-tui
mkdir -p "$launch_state_dir"
nohup "${launcher[@]}" "${qt_args[@]}" --launch "$instance_id" \
    </dev/null >>"$launch_state_dir/launcher.log" 2>&1 &
disown
