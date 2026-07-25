# UI Style Notes

## Update and deletion actions

- `Hard Update` is a destructive recovery action: it uses the warning/action treatment and always asks for confirmation before replacing the local GUI checkout.
- `Hard Delete` is a destructive removal action: it uses the same confirmation treatment and clearly states that Hyprland and dotfiles are not deleted.
- Both actions use the existing vertical action-stack layout and the existing dialog typography.
- Long command output is shown in the shared scrollable, monospace error dialog with a `Copy` action.
