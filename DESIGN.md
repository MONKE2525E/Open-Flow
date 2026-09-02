# Verenu UI Design System

This is the source of truth for shared frontend controls. Use it before adding or changing a button, dropdown, toggle, or control animation.

## Source of truth

| File | Owns |
| --- | --- |
| [`src/theme.css`](src/theme.css) | Themeable color, typography, radius, layout, and base elevation tokens. |
| [`src/ui.css`](src/ui.css) | Shared button states, dropdown anatomy, focus feedback, and CSS motion tokens. |
| [`src/lib/motion.ts`](src/lib/motion.ts) | Svelte transition timing, distance, reduced-motion behavior, and reusable transition helpers. |
| [`src/lib/components/Dropdown.svelte`](src/lib/components/Dropdown.svelte) | Headless outside-click and Escape handling for composed dropdowns. |
| [`src/lib/components/Toggle.svelte`](src/lib/components/Toggle.svelte) | The shared accessible switch control. The `toggle` class is a smoke-test contract. |
| [`docs/colors.md`](docs/colors.md) | Palette and typography reference. |

Do not define `.btn-primary`, `.btn-ghost`, `.btn-danger`, `.ui-dropdown-*`, or their basic interactive states inside a Svelte component. Add a semantic modifier only for layout or content that the primitive cannot know, such as a menu width or text truncation.

## Standard controls

### Buttons

```svelte
<!-- One obvious next action. -->
<button class="btn-primary" onclick={save}>Save changes</button>

<!-- A secondary, non-destructive action. -->
<button class="btn-ghost" onclick={cancel}>Cancel</button>

<!-- A destructive action. Use an explicit label. -->
<button class="btn-danger" onclick={deleteItem}>Delete history</button>

<!-- Use only when the surrounding control is intentionally compact. -->
<button class="btn-primary btn-compact" onclick={add}>Add</button>
```

| Variant | Use for | Shared behavior |
| --- | --- | --- |
| `btn-primary` | The main action in a local context, such as Save, Add, or Confirm. | Ink surface, strong contrast, hover, disabled state, focus ring. |
| `btn-ghost` | Secondary actions, utilities, external links, and dismiss actions. | Transparent surface, bordered hover state, disabled state, focus ring. |
| `btn-danger` | Confirmed destructive actions only. | Warning surface, filled destructive hover, disabled state, focus ring. |
| `btn-compact` | A space-constrained button that still has a complete label. | Reduces height, padding, and type size. It is a size modifier, not a visual variant. |

Do not use a standard button for navigation rows, selection cards, tabs, microphone recording, or the dictation pill. Those controls communicate different states and are intentionally specialized.

### Dropdowns

Use the structural classes on a feature-local dropdown. This centralizes visual treatment while allowing each feature to retain typed values, option descriptions, width constraints, and keyboard behavior.

```svelte
<script lang="ts">
  import Dropdown from '$lib/components/Dropdown.svelte';

  let open = $state(false);
</script>

<Dropdown bind:open closeSelector=".example-dropdown">
  <div class="ui-dropdown example-dropdown">
    <button
      class="btn-ghost ui-dropdown-trigger"
      aria-haspopup="listbox"
      aria-expanded={open}
      onclick={() => (open = !open)}
    >
      Current option
      <svg aria-hidden="true">...</svg>
    </button>

    {#if open}
      <div class="ui-dropdown-menu" role="listbox">
        <button class="ui-dropdown-option active" role="option" aria-selected="true">Current option</button>
      </div>
    {/if}
  </div>
</Dropdown>
```

| Class | Responsibility |
| --- | --- |
| `ui-dropdown` | Anchors a menu to its trigger. |
| `ui-dropdown-trigger` | Trigger shape, chevron motion, focus ring, and open treatment. Combine with `btn-ghost` when it is a secondary setting control. |
| `ui-dropdown-menu` | Popover surface, stacking, border, shadow, and scroll behavior. |
| `ui-dropdown-option` | Option row, selected state, hover, focus, and text overflow. |
| `ui-dropdown-menu--padded` | Use for compact choice lists that need separated rounded options rather than row dividers. |

`Dropdown.svelte` closes on outside click and Escape. Keep feature-local keyboard logic only when it has a real job, such as arrow-key listbox navigation or restoring focus to a particular trigger.

## Interaction and motion

| Token or helper | Value | Use |
| --- | --- | --- |
| `--ui-duration-fast` | `150ms` | Hover, focus, chevron rotation, button feedback. |
| `--ui-duration-base` | `220ms` | A shared CSS duration reserved for more deliberate control transitions. |
| `--ui-ease-out` | `cubic-bezier(0.22, 1, 0.36, 1)` | CSS control transitions. |
| `MOTION_MS.fast` | `150` | Short Svelte transitions. |
| `MOTION_MS.base` | `220` | Standard Svelte transitions. |
| `MOTION_MS.panel` | `280` | Menu and panel entrance transitions. |

Shared controls must use opacity, color, border, or transform for motion. Do not animate layout properties in CSS. `src/lib/motion.ts` scales transition duration and distance for users who prefer reduced motion; `src/ui.css` shortens its CSS durations to match.

## Current inventory

Audit performed 2026-08-05. The app contains 172 `<button>` elements, two visually-hidden native `<select>` elements that mirror custom listboxes, 98 static button class combinations, and 106 unique button class tokens. The list below assigns every current token to a design role so a new control can be matched to an existing role before another one is invented.

| Category | Existing classes or roles | Standardization status |
| --- | --- | --- |
| Shared actions | `btn-primary`, `btn-ghost`, `btn-danger`, `btn-compact`, `add-btn`, `btn-clear`, `cal-btn`, `load-older-btn`, `notification-test-send-button`, `simulation-provider-button` | The first four are global in `src/ui.css`. The remaining tokens are layout or feature modifiers on an action. |
| Dropdown anatomy | `ui-dropdown`, `ui-dropdown-trigger`, `ui-dropdown-menu`, `ui-dropdown-option`, `ui-dropdown-menu--padded`, `notification-test-item`, `simulation-provider-item` | Global in `src/ui.css`. Used by language, microphone, app mapping, transcription strategy, local-model memory policy, provider simulation, and notification-test menus. |
| Switches and recording | `toggle`, `mic-btn`, `hf-btn`, `cancel`, `confirm`, `badge`, `key-badge`, `keybind-btn`, `local-meta-toggle` | Shared components or flow-specific controls. Do not flatten recording states into a normal button. |
| Application navigation | `nav-item`, `settings-nav-item`, `settings-back`, `btn-back`, `tab`, `shot-nav`, `dot`, `dl-cancel` | Navigation-specific. Preserve smoke-test selectors. |
| List selections | `dict-row`, `snip-row`, `lang-row`, `app-picker-item`, `language-item`, `mic-item`, `models-dropdown-item`, `profile-drop-item`, `cleanup-drop-item`, `row-drop-item`, `mapping-badge-btn` | Selection controls. Inherit dropdown option styles where they are menu options. |
| Cards and accordions | `pick-card`, `env-card`, `provider-card`, `style-card`, `fork-card`, `tile-head`, `preset-select`, `state-pill`, `muted`, `card-btn`, `accent`, `ghost`, `flip-face`, `front`, `back` | Intentional feature-specific choices. They communicate a selected surface or disclosure state, not a generic action. |
| Inspector actions | `btn-insp-edit`, `btn-insp-delete`, `mapping-delete-btn`, `remove-dot` | Destructive or contextual row actions. Keep feature-local confirmation state. |
| Modal controls | `modal-backdrop`, `icon-btn`, `prompt-modal-close`, `prompt-btn`, `prompt-btn-ghost`, `snippet-nudge` | Modal mechanics and layout vary. Reuse standard action variants for modal footer actions. |
| Form and settings utilities | `appearance-option`, `accent-trigger`, `accent-swatch`, `badge key-badge keybind-btn`, `cleanup-off-link`, `language-btn`, `mic-btn`, `models-dropdown-btn`, `notification-test-dropdown-button`, `simulation-provider-button`, `profile-drop-btn`, `cleanup-drop-btn` | Dropdown triggers use the global anatomy. The accent picker is a semantic color control. Other controls remain semantic. |
| Setup and calibration | `btn-skip`, `cal-cancel-btn`, `cal-cancel-icon-btn`, `cal-recalibrate-btn`, `btn-open`, `btn-got-key`, `show-btn`, `help-btn`, `tryit-reset`, `lang-clear` | Setup flow has deliberate pacing and spatial constraints. Do not swap in a primary button without checking the flow. |
| Downloads and providers | `card-btn`, `accent`, `ghost`, `show-more-btn`, `custom-add-btn`, `custom-toggle-btn`, `simple-name`, `model-name`, `add-fallback-btn`, `preset-action-btn`, `prompt-edit-btn` | Feature-specific actions. Reassess only after three matching uses emerge. |
| Permissions and system actions | `perm-action`, `permission-details-toggle`, `permission-refresh-btn`, `btn-repair`, `version-tap`, `desc` | System and permission states need their own semantics and feedback. |
| History and feedback | `copy-btn`, `copy-logs-btn`, `retry-btn`, `load-warning-retry`, `load-older-btn`, `clear-btn`, `sort-pill`, `status-link`, `update-btn`, `update-dismiss`, `toast-close` | Feedback and one-off recovery actions. Prefer `btn-ghost` if a new use has no unique feedback state. |
| Native select mirrors | `profile-select profile-select-hidden`, `cleanup-select cleanup-select-hidden` | Accessibility mirrors for app-mapping custom listboxes. Keep synchronized with their visible dropdown choices. |

## Adoption rules

1. Start with `btn-primary`, `btn-ghost`, or `btn-danger` for a new action. Add `btn-compact` only when density requires it.
2. Start a new dropdown with the five `ui-dropdown` classes and the headless `Dropdown` controller. Give a feature its own behavior only when the selection logic needs it.
3. Before extracting another control, confirm the same semantic intent appears at least three times. Similar pixels are not enough.
4. Keep colors, radii, focus states, and core motion in `src/theme.css`, `src/ui.css`, or `src/lib/motion.ts`, never in a copied component rule.
5. Preserve selectors named in `CLAUDE.md`'s smoke-test contract, even when their visual treatment moves into a shared primitive.
