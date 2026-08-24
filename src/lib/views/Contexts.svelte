<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { fly, slide } from 'svelte/transition';
  import { crossfade } from 'svelte/transition';
  import { flip } from 'svelte/animate';
  import { cubicOut, expoOut } from 'svelte/easing';
  import { emit, invoke, listen } from '../tauri';
  import { modalBackdrop, modalCard, MOTION_MS, MOTION_PX, motionMs, motionPx, pageSwap, directionFromOrder } from '../motion';
  import { modalFocusTrap } from '../modalFocus';
  import { classifyIpcError } from '../errors';
  import {
    type Context,
    type ContextTarget,
    type ContextWebsiteTarget,
    type DictionaryEntry,
    type Snippet,
  } from '../stores';
  import {
    cleanAppName,
    cleanupIntensityOptions,
    getCleanupIntensityLabel,
    getProfileLabel,
    normalizeExe,
    profileOptions,
    type InstalledApp,
  } from '../appMappings';
  import { icons } from '../icons';
  import {
    contextsStore,
    loadContexts as loadSharedContexts,
    EVERYWHERE_ID,
  } from '../contextsStore.svelte';
  import AppIcon from '../components/AppIcon.svelte';
  import SiteIcon from '../components/SiteIcon.svelte';
  import Toggle from '../components/Toggle.svelte';
  import { matchesAppSearch } from '../components/appMappings/helpers';
  import { handleListboxOptionKeydown, focusListboxOption } from '../components/appMappings/listbox';
  import DictionaryModal from './dictionary/DictionaryModal.svelte';
  import SnippetModal from './snippets/SnippetModal.svelte';
  import { fmtDate as fmtDictionaryDate, confidenceLabel } from './dictionary/helpers';
  import { fmtDate as fmtSnippetDate } from './snippets/helpers';

  const CONTEXT_NAME_MAX_LENGTH = 30;
  const CONTEXT_CUSTOM_INSTRUCTIONS_MAX_LENGTH = 300;
  const CONTEXT_TABS = [
    { id: 'vocabulary', label: 'Vocabulary' },
    { id: 'snippets', label: 'Snippets' },
  ] as const;
  const CONTEXT_TAB_ORDER = CONTEXT_TABS.map((t) => t.id);
  type ContextTab = (typeof CONTEXT_TABS)[number]['id'];

  // Curated subset of the shared icon set — enough variety for work modes
  // without dumping every icon in the app into a picker grid.
  // Chosen for relevance to the "work mode" contexts people actually create
  // (coding, browsing, chat/support, writing, dictation, etc.) rather than
  // reusing whatever settings/nav icons happened to already exist.
  const CONTEXT_ICON_CHOICES = ['code', 'browser', 'chat', 'pencil', 'mic', 'book', 'sliders', 'shield', 'key', 'chart', 'lock', 'bell'] as const;

  // A small curated swatch, not a full picker — same lightness/chroma family
  // as the app's own accent (oklch L~0.65-0.72, C~0.09-0.13) so every option
  // reads as "part of this UI" rather than a clashing sticker color. Orange
  // reuses the live --accent token so it always matches the theme exactly;
  // the rest are fixed OKLCH values chosen to sit at roughly the same
  // lightness/chroma as the accent (same tonal family), spread across hue at
  // a comfortable amount of separation.
  const CONTEXT_COLOR_CHOICES = [
    { id: 'orange', label: 'Orange', value: 'var(--accent)' },
    { id: 'sage', label: 'Sage', value: 'oklch(0.65 0.09 150)' },
    { id: 'blue', label: 'Blue', value: 'oklch(0.65 0.09 250)' },
    { id: 'mauve', label: 'Mauve', value: 'oklch(0.63 0.09 350)' },
    { id: 'gold', label: 'Gold', value: 'oklch(0.72 0.11 90)' },
  ] as const;
  const MODAL_APP_MATCH_LIMIT = 40;
  // Mirrors `MAX_USER_CONTEXTS` in src-tauri/src/data/db/contexts.rs — the
  // backend is the source of truth and enforces this too; this just avoids a
  // round-trip error for the common "hit the limit" case.
  const MAX_USER_CONTEXTS = 200;

  const [send, receive] = crossfade({ duration: motionMs(MOTION_MS.base), easing: expoOut });

  // The context list, its targets and the current selection are shared with
  // the sidebar, which is where contexts are now navigated and managed from.
  const contexts = $derived(contextsStore.contexts);
  const targets = $derived(contextsStore.targets);
  const websites = $derived(contextsStore.websites);
  const selectedContextId = $derived(contextsStore.selectedId);
  let installedApps = $state<InstalledApp[]>([]);
  let dictionary = $state<DictionaryEntry[]>([]);
  let snippets = $state<Snippet[]>([]);
  let search = $state('');
  let tab = $state<ContextTab>('vocabulary');
  let tabDir = $state<1 | -1>(1);
  let tablistEl = $state<HTMLDivElement | null>(null);
  let sort = $state<'newest' | 'alpha'>('newest');
  // "Selected" here means "open in the edit modal", not "shown in an
  // inspector panel" — Contexts uses a three-dot row menu (Edit/Delete)
  // instead of a click-to-inspect panel.
  let selectedDictionary = $state<DictionaryEntry | null>(null);
  let selectedSnippet = $state<Snippet | null>(null);
  let openRowMenu = $state<{ kind: 'dictionary' | 'snippet'; id: number } | null>(null);
  let rowMenuPos = $state<{ top: number; right: number } | null>(null);
  let rowMenuDeleteArmed = $state(false);
  let rowMenuShowMove = $state(false);
  let modal = $state<'context' | 'dictionary' | 'snippet' | null>(null);
  let contextModalMode = $state<'create' | 'edit'>('create');
  let contextName = $state('');
  let contextError = $state('');
  let savingContext = $state(false);
  let contextInput = $state<HTMLInputElement | null>(null);
  let modalAppQuery = $state('');
  let modalAppInputEl = $state<HTMLInputElement | null>(null);
  let modalAppMatchPos = $state<{ top: number; left: number; width: number } | null>(null);
  let modalAppHighlight = $state(0);
  let modalApps = $state<InstalledApp[]>([]);
  let modalWebsiteInput = $state('');
  let modalWebsites = $state<string[]>([]);
  let modalIcon = $state<string | null>(null);
  let modalColor = $state<string | null>(null);
  let modalColorPickerOpen = $state(false);
  let modalColorPickerPos = $state<{ top: number; left: number } | null>(null);
  let modalTone = $state<string | null>(null);
  let modalCleanupIntensity = $state<string | null>(null);
  let modalCustomInstructions = $state('');
  let modalContextualFormattingDisabled = $state(false);
  let modalAdvancedOpen = $state(false);
  let editingContextId = $state<number | null>(null);
  // Tone/cleanup dropdown menus render fixed-position at the top level (not
  // nested in the modal card) because the modal card and body both clip
  // overflow — an absolutely-positioned menu inside them gets cut off.
  let openFieldMenu = $state<'tone' | 'cleanup' | null>(null);
  let fieldMenuPos = $state<{ top: number; left: number; width: number } | null>(null);
  let toneTrigger = $state<HTMLButtonElement | null>(null);
  let cleanupTrigger = $state<HTMLButtonElement | null>(null);
  let appPickerOpen = $state(false);
  let appPickerQuery = $state('');
  let appPickerInput = $state<HTMLInputElement | null>(null);
  let appPickerTrigger = $state<HTMLButtonElement | null>(null);
  const appPickerMenuId = 'context-app-picker-list';
  let websitePickerOpen = $state(false);
  let websiteInput = $state('');
  let websiteInputEl = $state<HTMLInputElement | null>(null);
  let websitePickerTrigger = $state<HTMLButtonElement | null>(null);
  let websiteError = $state('');
  let contextErrorMessage = $state('');
  let loading = $state(true);
  let loadToken = 0;

  // Marks a chip (by exe or domain) as "just added by the user" so its
  // pop-in transition only plays for a genuinely new addition — not for the
  // whole list re-rendering when switching to a different context group.
  let recentlyAddedChips = $state<Set<string>>(new Set());
  function markRecentlyAdded(key: string) {
    recentlyAddedChips = new Set(recentlyAddedChips).add(key);
    window.setTimeout(() => {
      recentlyAddedChips = new Set([...recentlyAddedChips].filter((k) => k !== key));
    }, 320);
  }

  let checkingDomain = $state(false);

  const atContextLimit = $derived(contexts.filter((c) => !c.is_everywhere).length >= MAX_USER_CONTEXTS);
  const selectedContext = $derived(
    contexts.find((context) => context.id === selectedContextId) ?? contexts[0] ?? null,
  );
  const selectedTargets = $derived(targets.filter((target) => target.context_id === selectedContextId));
  const selectedWebsites = $derived(websites.filter((site) => site.context_id === selectedContextId));
  const assignedExes = $derived(new Set(targets.map((target) => normalizeExe(target.executable))));
  const availableApps = $derived(
    installedApps.filter((app) => !assignedExes.has(normalizeExe(app.exe))),
  );
  const appPickerMatches = $derived(
    availableApps.filter((app) => matchesAppSearch(app, appPickerQuery)).slice(0, 40),
  );
  const modalPickedExes = $derived(new Set(modalApps.map((app) => normalizeExe(app.exe))));
  const modalAppMatches = $derived(
    modalAppQuery.trim()
      ? availableApps.filter((app) => !modalPickedExes.has(normalizeExe(app.exe)) && matchesAppSearch(app, modalAppQuery)).slice(0, MODAL_APP_MATCH_LIMIT)
      : [],
  );
  const modalWebsitePreview = $derived(normalizeDomainInput(modalWebsiteInput));
  const modalWebsiteValid = $derived(modalWebsitePreview.length > 0 && isLikelyValidDomain(modalWebsitePreview));
  const websitePreview = $derived(normalizeDomainInput(websiteInput));
  const websiteValid = $derived(websitePreview.length > 0 && isLikelyValidDomain(websitePreview));
  const filteredDictionary = $derived.by(() => {
    const q = search.trim().toLowerCase();
    let list = q
      ? dictionary.filter((entry) => entry.term.toLowerCase().includes(q) || (entry.mistake ?? '').toLowerCase().includes(q))
      : [...dictionary];
    if (sort === 'newest') list.sort((a, b) => b.created_at.localeCompare(a.created_at));
    if (sort === 'alpha') list.sort((a, b) => a.term.localeCompare(b.term));
    return list;
  });

  const filteredSnippets = $derived.by(() => {
    const q = search.trim().toLowerCase();
    let list = q
      ? snippets.filter((snippet) => snippet.trigger.toLowerCase().includes(q) || snippet.expansion.toLowerCase().includes(q))
      : [...snippets];
    if (sort === 'newest') list.sort((a, b) => b.created_at.localeCompare(a.created_at));
    if (sort === 'alpha') list.sort((a, b) => a.trigger.localeCompare(b.trigger));
    return list;
  });

  // A compact usage line, not an analytics panel — deeper context analytics
  // belong in Insights. Counts only dictations recorded since the schema
  // started attributing them to a context, so it stays empty on fresh installs.
  type ContextStats = { dictations: number; words: number; last_used_at: string | null };
  let contextStats = $state<ContextStats | null>(null);

  async function loadContextStats(contextId: number) {
    const token = loadToken;
    try {
      const stats = await invoke<ContextStats>('get_context_stats', { contextId });
      if (token !== loadToken) return;
      contextStats = stats ?? null;
    } catch {
      // Cosmetic — a failed stats read just hides the line.
      if (token === loadToken) contextStats = null;
    }
  }

  async function loadContextItems(contextId: number) {
    const token = ++loadToken;
    loading = true;
    contextErrorMessage = '';
    try {
      const [nextDictionary, nextSnippets] = await Promise.all([
        invoke<DictionaryEntry[]>('get_context_dictionary', { contextId }),
        invoke<Snippet[]>('get_context_snippets', { contextId }),
      ]);
      if (token !== loadToken) return;
      dictionary = nextDictionary ?? [];
      snippets = nextSnippets ?? [];
      void loadContextStats(contextId);
      selectedDictionary = null;
      selectedSnippet = null;
      closeRowMenu();
    } catch (error) {
      if (token !== loadToken) return;
      contextErrorMessage = classifyIpcError(error).message;
    } finally {
      if (token === loadToken) loading = false;
    }
  }

  async function loadContexts() {
    try {
      const [nextApps] = await Promise.all([
        invoke<InstalledApp[]>('get_installed_apps'),
        // Force a refresh: the sidebar loads this once at startup, but this
        // page is the surface where targets get added and removed.
        loadSharedContexts(true),
      ]);
      installedApps = (nextApps ?? []).map((app) => ({
        name: cleanAppName(app.name || app.exe),
        exe: normalizeExe(app.exe),
      }));
      if (contextsStore.error) contextErrorMessage = contextsStore.error;
      await loadContextItems(selectedContextId);
    } catch (error) {
      contextErrorMessage = classifyIpcError(error).message;
      loading = false;
    }
  }

  onMount(() => {
    void loadContexts();
    let stop: (() => void) | undefined;
    listen('verenu:dictionary-updated', () => void loadContextItems(selectedContextId))
      .then((unlisten) => { stop = unlisten; })
      .catch(() => {});
    return () => stop?.();
  });

  $effect(() => {
    selectedContextId;
    if (contexts.length > 0) void loadContextItems(selectedContextId);
  });

  // The sidebar owns the context list but not the context form, so it asks
  // this page to open the same create/edit modal it always had.
  $effect(() => {
    const request = contextsStore.modalRequest;
    if (!request) return;
    contextsStore.modalRequest = null;
    if (request.mode === 'edit') contextsStore.selectedId = request.id;
    openContextModal(request.mode, request.mode === 'edit' ? request.id : undefined);
  });

  function closeAppPicker() {
    appPickerOpen = false;
    appPickerQuery = '';
  }

  $effect(() => {
    if (!appPickerOpen) return;
    const handleClose = (event: Event) => {
      if (event.target instanceof Element && event.target.closest('.app-picker, .app-picker-trigger')) return;
      closeAppPicker();
    };
    const timeout = window.setTimeout(() => {
      window.addEventListener('pointerdown', handleClose);
      window.addEventListener('scroll', handleClose, { capture: true, passive: true });
    });
    return () => {
      window.clearTimeout(timeout);
      window.removeEventListener('pointerdown', handleClose);
      window.removeEventListener('scroll', handleClose, { capture: true });
    };
  });

  $effect(() => {
    if (!websitePickerOpen) return;
    const handleClose = (event: Event) => {
      if (event.target instanceof Element && event.target.closest('.app-picker, .app-picker-trigger')) return;
      closeWebsitePicker();
    };
    const timeout = window.setTimeout(() => {
      window.addEventListener('pointerdown', handleClose);
      window.addEventListener('scroll', handleClose, { capture: true, passive: true });
    });
    return () => {
      window.clearTimeout(timeout);
      window.removeEventListener('pointerdown', handleClose);
      window.removeEventListener('scroll', handleClose, { capture: true });
    };
  });

  function selectTab(id: ContextTab) {
    if (id === tab) return;
    tabDir = directionFromOrder(tab, id, CONTEXT_TAB_ORDER);
    tab = id;
    search = '';
  }

  function handleTablistKeydown(event: KeyboardEvent) {
    const tabButtons = tablistEl?.querySelectorAll<HTMLButtonElement>('.tab') ?? [];
    if (tabButtons.length === 0) return;
    const index = Array.from(tabButtons).indexOf(document.activeElement as HTMLButtonElement);
    if (index === -1) return;

    let next: number | null = null;
    if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = tabButtons.length - 1;
    else if (event.key === 'ArrowLeft') next = (index - 1 + tabButtons.length) % tabButtons.length;
    else if (event.key === 'ArrowRight') next = (index + 1) % tabButtons.length;
    if (next === null) return;

    event.preventDefault();
    const target = tabButtons[next];
    target.focus();
    const id = target.id.replace('context-tab-', '') as ContextTab;
    if (id !== tab) selectTab(id);
  }

  function closeRowMenu() {
    openRowMenu = null;
    rowMenuPos = null;
    rowMenuDeleteArmed = false;
    rowMenuShowMove = false;
  }

  function toggleRowMenu(kind: 'dictionary' | 'snippet', id: number, trigger: HTMLButtonElement | null) {
    if (openRowMenu?.kind === kind && openRowMenu.id === id) {
      closeRowMenu();
      return;
    }
    if (!trigger) return;
    const rect = trigger.getBoundingClientRect();
    rowMenuPos = { top: rect.bottom + 4, right: window.innerWidth - rect.right };
    openRowMenu = { kind, id };
    rowMenuDeleteArmed = false;
    rowMenuShowMove = false;
  }

  async function moveRowItem(kind: 'dictionary' | 'snippet', id: number, targetContextId: number) {
    try {
      const assignmentCommand = kind === 'dictionary'
        ? 'set_dictionary_context_assignment'
        : 'set_snippet_context_assignment';
      const idKey = kind === 'dictionary' ? 'dictionaryId' : 'snippetId';
      await invoke(assignmentCommand, { contextId: targetContextId, [idKey]: id, assigned: true });
      await invoke(assignmentCommand, { contextId: selectedContextId, [idKey]: id, assigned: false });
      if (kind === 'dictionary') {
        dictionary = dictionary.filter((entry) => entry.id !== id);
      } else {
        snippets = snippets.filter((snippet) => snippet.id !== id);
      }
    } catch (error) {
      contextErrorMessage = classifyIpcError(error).message;
    } finally {
      closeRowMenu();
    }
  }

  // Clicking anywhere on the row opens the same menu the kebab does — the
  // row used to be inert outside the kebab's small hit area, which read as
  // "clicking it does nothing". Bail out when the click originated on the
  // kebab itself so its own handler doesn't get double-fired (open then
  // immediately close again).
  function handleItemRowClick(kind: 'dictionary' | 'snippet', id: number, event: MouseEvent) {
    if (event.target instanceof Element && event.target.closest('.item-kebab')) return;
    const trigger = (event.currentTarget as HTMLElement).querySelector<HTMLButtonElement>('.item-kebab');
    toggleRowMenu(kind, id, trigger);
  }

  $effect(() => {
    if (!openRowMenu) return;
    // Exclude clicks on the kebab trigger itself — otherwise the outside-click
    // close fires on pointerdown, then the trigger's own click handler runs
    // right after and reopens it, which looks like the menu never closed at
    // all (just replayed its opening animation).
    const handleClose = (event: Event) => {
      if (event.target instanceof Element && event.target.closest('.item-kebab')) return;
      closeRowMenu();
    };
    const timeout = window.setTimeout(() => {
      window.addEventListener('pointerdown', handleClose);
      window.addEventListener('scroll', handleClose, { capture: true, passive: true });
    });
    return () => {
      window.clearTimeout(timeout);
      window.removeEventListener('pointerdown', handleClose);
      window.removeEventListener('scroll', handleClose, { capture: true });
    };
  });

  function openAddDictionary() {
    selectedDictionary = null;
    selectedSnippet = null;
    modal = 'dictionary';
  }

  function openAddSnippet() {
    selectedDictionary = null;
    selectedSnippet = null;
    modal = 'snippet';
  }

  function handleDictionarySaved(entry: DictionaryEntry) {
    dictionary = [entry, ...dictionary.filter((item) => item.id !== entry.id)];
    selectedDictionary = entry;
    void finishNewAssignment('dictionary', entry.id);
  }

  function handleSnippetSaved(snippet: Snippet) {
    snippets = [snippet, ...snippets.filter((item) => item.id !== snippet.id)];
    selectedSnippet = snippet;
    void finishNewAssignment('snippet', snippet.id);
  }

  // On create, the backend already assigns the new (or found-and-reused)
  // entry to `selectedContextId` directly — this only covers the edit path,
  // ensuring an entry edited while viewing a context stays linked to it.
  // Unlike the old version, it no longer strips other context assignments:
  // a term can belong to more than one context at once.
  async function finishNewAssignment(kind: 'dictionary' | 'snippet', id: number) {
    if (selectedContextId === EVERYWHERE_ID) return;
    try {
      const assignmentCommand = kind === 'dictionary'
        ? 'set_dictionary_context_assignment'
        : 'set_snippet_context_assignment';
      const idKey = kind === 'dictionary' ? 'dictionaryId' : 'snippetId';
      await invoke(assignmentCommand, { contextId: selectedContextId, [idKey]: id, assigned: true });
    } catch (error) {
      contextErrorMessage = classifyIpcError(error).message;
    }
  }

  function editDictionary(entry: DictionaryEntry) {
    closeRowMenu();
    selectedDictionary = entry;
    modal = 'dictionary';
  }

  function editSnippet(snippet: Snippet) {
    closeRowMenu();
    selectedSnippet = snippet;
    modal = 'snippet';
  }

  async function confirmDeleteRow(kind: 'dictionary' | 'snippet', id: number) {
    if (!rowMenuDeleteArmed) {
      rowMenuDeleteArmed = true;
      return;
    }
    try {
      await invoke(kind === 'dictionary' ? 'remove_dictionary_entry' : 'remove_snippet', { id });
      if (kind === 'dictionary') {
        dictionary = dictionary.filter((entry) => entry.id !== id);
      } else {
        snippets = snippets.filter((snippet) => snippet.id !== id);
      }
    } catch (error) {
      contextErrorMessage = classifyIpcError(error).message;
    } finally {
      closeRowMenu();
    }
  }

  function openContextModal(mode: 'create' | 'edit' = 'create', targetId?: number) {
    if (mode === 'create' && atContextLimit) return;
    contextModalMode = mode;
    // Resolve the edit target explicitly from the requested id: going through
    // the selectedContext derived would silently fall back to contexts[0]
    // when the id isn't (yet) in the list, prefilled with the wrong context.
    const editing = mode === 'edit'
      ? contexts.find((context) => context.id === (targetId ?? contextsStore.selectedId)) ?? null
      : null;
    // Save uses this id directly so it can never drift to a different
    // context if the list or selection changes while the modal is open.
    editingContextId = editing?.id ?? null;
    contextName = editing?.name ?? '';
    modalIcon = editing?.icon ?? null;
    modalColor = editing?.color ?? null;
    closeModalColorPicker();
    modalTone = editing?.tone ?? null;
    modalCleanupIntensity = editing?.cleanup_intensity ?? null;
    modalCustomInstructions = editing?.custom_instructions ?? '';
    modalContextualFormattingDisabled = editing?.contextual_formatting_disabled ?? false;
    modalAdvancedOpen = false;
    modalApps = [];
    modalAppQuery = '';
    modalWebsites = [];
    modalWebsiteInput = '';
    modalWebsiteError = '';
    contextError = '';
    closeFieldMenu();
    modal = 'context';
  }

  function closeFieldMenu() {
    openFieldMenu = null;
    fieldMenuPos = null;
  }

  function toggleFieldMenu(which: 'tone' | 'cleanup', trigger: HTMLButtonElement | null) {
    if (openFieldMenu === which) {
      closeFieldMenu();
      return;
    }
    if (!trigger) return;
    const rect = trigger.getBoundingClientRect();
    fieldMenuPos = { top: rect.bottom + 4, left: rect.left, width: rect.width };
    openFieldMenu = which;
  }

  $effect(() => {
    if (!openFieldMenu) return;
    const handleClose = (event: Event) => {
      if (event.target instanceof Element && event.target.closest('.ui-dropdown-trigger')) return;
      closeFieldMenu();
    };
    const timeout = window.setTimeout(() => {
      window.addEventListener('pointerdown', handleClose);
      window.addEventListener('scroll', handleClose, { capture: true, passive: true });
    });
    return () => {
      window.clearTimeout(timeout);
      window.removeEventListener('pointerdown', handleClose);
      window.removeEventListener('scroll', handleClose, { capture: true });
    };
  });

  // The results list renders fixed-position (outside the modal card, see
  // template) so it overlays the fields below it instead of pushing the
  // modal taller — the modal card clips/scrolls its own content, so an
  // absolutely-positioned menu inside it would be cut off (same issue the
  // Tone/Cleanup dropdowns had).
  function updateModalAppMatchPos() {
    if (!modalAppInputEl) return;
    const rect = modalAppInputEl.getBoundingClientRect();
    modalAppMatchPos = { top: rect.bottom + 4, left: rect.left, width: rect.width };
  }

  $effect(() => {
    if (!modalAppQuery.trim()) return;
    const handleClose = (event: PointerEvent) => {
      if (event.target instanceof Element && event.target.closest('#context-app')) return;
      modalAppQuery = '';
    };
    const timeout = window.setTimeout(() => {
      window.addEventListener('pointerdown', handleClose);
    });
    return () => {
      window.clearTimeout(timeout);
      window.removeEventListener('pointerdown', handleClose);
    };
  });

  function pickModalApp(app: InstalledApp) {
    if (modalPickedExes.has(normalizeExe(app.exe))) return;
    modalApps = [...modalApps, app];
    markRecentlyAdded(normalizeExe(app.exe));
    modalAppQuery = '';
    modalAppHighlight = 0;
  }

  // Reset the highlighted row whenever the match list changes shape, so a
  // stale index from a longer previous list can't point past the new end.
  $effect(() => {
    modalAppMatches;
    modalAppHighlight = 0;
  });

  function handleModalAppKeydown(event: KeyboardEvent) {
    if (!modalAppQuery.trim() || modalAppMatches.length === 0) return;
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      modalAppHighlight = (modalAppHighlight + 1) % modalAppMatches.length;
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      modalAppHighlight = (modalAppHighlight - 1 + modalAppMatches.length) % modalAppMatches.length;
    } else if (event.key === 'Enter') {
      event.preventDefault();
      const app = modalAppMatches[modalAppHighlight];
      if (app) pickModalApp(app);
    } else if (event.key === 'Escape') {
      modalAppQuery = '';
    }
  }

  function removeModalApp(exe: string) {
    modalApps = modalApps.filter((app) => normalizeExe(app.exe) !== normalizeExe(exe));
  }

  let modalWebsiteError = $state('');

  async function addModalWebsite() {
    if (checkingDomain) return;
    const domain = normalizeDomainInput(modalWebsiteInput);
    if (!domain || !isLikelyValidDomain(domain)) return;
    if (modalWebsites.includes(domain)) {
      modalWebsiteInput = '';
      return;
    }
    modalWebsiteError = '';
    checkingDomain = true;
    try {
      const exists = await invoke<boolean>('check_domain_exists', { domain });
      if (!exists) {
        modalWebsiteError = "This domain doesn't seem to exist.";
        return;
      }
    } catch {
      // A failed check (not a resolved "doesn't exist") shouldn't block the
      // user — fail open rather than punish them for a flaky local network.
    } finally {
      checkingDomain = false;
    }
    modalWebsites = [...modalWebsites, domain];
    markRecentlyAdded(domain);
    modalWebsiteInput = '';
  }

  function removeModalWebsite(domain: string) {
    modalWebsites = modalWebsites.filter((item) => item !== domain);
  }

  function normalizeDomainInput(raw: string): string {
    const trimmed = raw.trim().toLowerCase();
    if (!trimmed) return '';
    const withoutScheme = trimmed.split('://').pop() ?? trimmed;
    return withoutScheme.split(/[/?#]/)[0]?.split('@').pop()?.split(':')[0] ?? '';
  }

  const DOMAIN_PATTERN = /^([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,}$/;

  function isLikelyValidDomain(domain: string): boolean {
    return DOMAIN_PATTERN.test(domain);
  }

  async function saveContext() {
    const name = contextName.trim();
    if (!name) {
      contextError = 'Give this context a name.';
      return;
    }
    let createdContextId: number | null = null;
    const editing = contextModalMode === 'edit' && editingContextId !== null
      ? contexts.find((context) => context.id === editingContextId) ?? null
      : null;
    if (contextModalMode === 'edit' && !editing) {
      // The context was deleted (or never resolved) while the modal was
      // open - refuse rather than silently falling through to create.
      contextError = 'This context no longer exists.';
      return;
    }
    savingContext = true;
    contextError = '';
    try {
      if (editing) {
        await invoke('update_context', { contextId: editing.id, name });
        await invoke('update_context_settings', {
          contextId: editing.id,
          icon: modalIcon,
          tone: modalTone,
          cleanupIntensity: modalCleanupIntensity,
          customInstructions: modalCustomInstructions.trim() || null,
          contextualFormattingDisabled: modalContextualFormattingDisabled,
        });
        if (modalColor !== editing.color) {
          await invoke('update_context_color', { contextId: editing.id, color: modalColor });
        }
        contextsStore.contexts = contexts.map((context) => context.id === editing.id
          ? { ...context, name, icon: modalIcon, tone: modalTone, cleanup_intensity: modalCleanupIntensity, custom_instructions: modalCustomInstructions.trim() || null, contextual_formatting_disabled: modalContextualFormattingDisabled, color: modalColor, updated_at: new Date().toISOString() }
          : context);
      } else {
        const created = await invoke<Context>('create_context', {
          name,
          icon: modalIcon,
          tone: modalTone,
          cleanupIntensity: modalCleanupIntensity,
          customInstructions: modalCustomInstructions.trim() || null,
          contextualFormattingDisabled: modalContextualFormattingDisabled,
        });
        createdContextId = created.id;
        if (modalColor) {
          await invoke('update_context_color', { contextId: created.id, color: modalColor });
          created.color = modalColor;
        }
        contextsStore.contexts = [...contexts, created];
        contextsStore.selectedId = created.id;
        for (const app of modalApps) {
          const target = await invoke<ContextTarget>('assign_context_target', {
            contextId: created.id,
            executable: app.exe,
          });
          // Read the live store array each iteration: a captured snapshot
          // would drop targets assigned by earlier iterations of this loop.
          contextsStore.targets = [...contextsStore.targets.filter((item) => normalizeExe(item.executable) !== normalizeExe(app.exe)), target];
        }
        for (const domain of modalWebsites) {
          const site = await invoke<ContextWebsiteTarget>('assign_context_website', {
            contextId: created.id,
            domain,
          });
          contextsStore.websites = [...contextsStore.websites.filter((item) => item.domain !== domain), site];
        }
      }
      modal = null;
    } catch (error) {
      if (createdContextId !== null) {
        contextsStore.selectedId = createdContextId;
        contextModalMode = 'edit';
      }
      contextError = classifyIpcError(error).message;
    } finally {
      savingContext = false;
    }
  }

  // Same picker, but for a context still being created/edited in the modal —
  // there's no context row to right-click yet, so this sets local modal
  // state instead of calling update_context_color directly; saveContext()
  // persists it alongside the rest of the form.
  function closeModalColorPicker() {
    modalColorPickerOpen = false;
    modalColorPickerPos = null;
  }

  function openModalColorPicker(event: MouseEvent) {
    event.preventDefault();
    modalColorPickerPos = { top: event.clientY + 4, left: event.clientX };
    modalColorPickerOpen = true;
  }

  function pickModalColor(color: string | null) {
    modalColor = color;
    closeModalColorPicker();
  }

  $effect(() => {
    if (!modalColorPickerOpen) return;
    const handleClose = () => closeModalColorPicker();
    const timeout = window.setTimeout(() => {
      window.addEventListener('pointerdown', handleClose);
      window.addEventListener('scroll', handleClose, { capture: true, passive: true });
    });
    return () => {
      window.clearTimeout(timeout);
      window.removeEventListener('pointerdown', handleClose);
      window.removeEventListener('scroll', handleClose, { capture: true });
    };
  });

  async function toggleAppPicker() {
    if (appPickerOpen) {
      closeAppPicker();
      return;
    }
    closeWebsitePicker();
    appPickerOpen = true;
    await tick();
    appPickerInput?.focus();
  }

  function handleAppPickerInputKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      closeAppPicker();
      appPickerTrigger?.focus();
      return;
    }
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      void focusListboxOption(appPickerMenuId);
    }
  }

  async function assignApp(app: InstalledApp) {
    if (!selectedContext || selectedContext.is_everywhere) return;
    try {
      const target = await invoke<ContextTarget>('assign_context_target', {
        contextId: selectedContext.id,
        executable: app.exe,
      });
      contextsStore.targets = [...targets.filter((item) => normalizeExe(item.executable) !== normalizeExe(app.exe)), target];
      markRecentlyAdded(normalizeExe(app.exe));
      closeAppPicker();
    } catch (error) {
      contextErrorMessage = classifyIpcError(error).message;
    }
  }

  async function removeApp(target: ContextTarget) {
    try {
      await invoke('remove_context_target', { contextId: target.context_id, executable: target.executable });
      contextsStore.targets = targets.filter((item) => item.id !== target.id);
    } catch (error) {
      contextErrorMessage = classifyIpcError(error).message;
    }
  }

  function appLabel(executable: string) {
    const app = installedApps.find((item) => normalizeExe(item.exe) === normalizeExe(executable));
    return cleanAppName(app?.name || executable);
  }

  function closeWebsitePicker() {
    websitePickerOpen = false;
    websiteInput = '';
    websiteError = '';
  }

  async function toggleWebsitePicker() {
    if (websitePickerOpen) {
      closeWebsitePicker();
      return;
    }
    closeAppPicker();
    websitePickerOpen = true;
    await tick();
    websiteInputEl?.focus();
  }

  async function assignWebsite() {
    if (checkingDomain) return;
    if (!selectedContext || selectedContext.is_everywhere) return;
    const domain = normalizeDomainInput(websiteInput);
    if (!domain || !isLikelyValidDomain(domain)) {
      websiteError = 'Enter a domain, e.g. mail.google.com';
      return;
    }
    websiteError = '';
    checkingDomain = true;
    try {
      const exists = await invoke<boolean>('check_domain_exists', { domain });
      if (!exists) {
        websiteError = "This domain doesn't seem to exist.";
        return;
      }
    } catch {
      // Fail open — see addModalWebsite for why.
    } finally {
      checkingDomain = false;
    }
    try {
      const site = await invoke<ContextWebsiteTarget>('assign_context_website', {
        contextId: selectedContext.id,
        domain,
      });
      contextsStore.websites = [...websites.filter((item) => item.domain !== domain), site];
      markRecentlyAdded(domain);
      closeWebsitePicker();
    } catch (error) {
      websiteError = classifyIpcError(error).message;
    }
  }

  function handleWebsiteInputKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      closeWebsitePicker();
      websitePickerTrigger?.focus();
      return;
    }
    if (event.key === 'Enter') {
      event.preventDefault();
      void assignWebsite();
    }
  }

  async function removeWebsite(site: ContextWebsiteTarget) {
    try {
      await invoke('remove_context_website', { contextId: site.context_id, domain: site.domain });
      contextsStore.websites = websites.filter((item) => item.id !== site.id);
    } catch (error) {
      contextErrorMessage = classifyIpcError(error).message;
    }
  }

  function closeModal() {
    modal = null;
    contextError = '';
    closeModalColorPicker();
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      closeRowMenu();
      closeAppPicker();
      closeWebsitePicker();
      closeFieldMenu();
      closeModalColorPicker();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="content-inner contexts-page" class:modal-open={modal !== null}>
  {#if contextErrorMessage}
    <div class="load-warning" role="alert">
      <span>{contextErrorMessage}</span>
      <button class="btn-ghost" type="button" onclick={() => contextErrorMessage = ''}>Dismiss</button>
    </div>
  {/if}

  <div class="contexts-shell">
    <main class="context-main">
      {#key selectedContextId}
      {#if selectedContext}
        <!--
          Swaps along the sidebar's own axis and in the direction the selection
          travelled, so the view reads as coming from the row that was clicked.
          The two panes overlap in one grid cell (see .context-main), so the
          incoming one never gets pushed below the outgoing one mid-swap.

          |global is required, not decorative: the element sits inside an {#if}
          whose condition never changes, so a local transition sees its own
          block as unchanged and skips entirely. It is the {#key} above that
          swaps, which only a global transition reacts to.
        -->
        <div
          class="context-content"
          in:pageSwap|global={{ axis: 'y', distance: contextsStore.selectDir * motionPx(MOTION_PX.page), duration: motionMs(MOTION_MS.panel) }}
          out:pageSwap|global={{ axis: 'y', distance: -contextsStore.selectDir * motionPx(MOTION_PX.lift), duration: motionMs(MOTION_MS.base) }}
        >
        <div class="context-header">
          <div>
            <h2>{selectedContext.name}</h2>
            <p>{selectedContext.is_everywhere ? 'These items are used when no specific app context group is active.' : 'These items are used when this context group is active.'}</p>
          </div>
          {#if !selectedContext.is_everywhere}
            <div class="context-actions">
              <button class="btn-ghost btn-compact app-picker-trigger" type="button" bind:this={appPickerTrigger} onclick={() => void toggleAppPicker()} aria-expanded={appPickerOpen} aria-haspopup="listbox">
                Add app
                <svg class="ui-chevron" width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>
              </button>
              {#if appPickerOpen}
                <div class="app-picker ui-dropdown-menu" role="presentation" onpointerdown={(event) => event.stopPropagation()} in:fly={{ y: motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.fast) }} out:fly={{ y: motionPx(MOTION_PX.nudge) * 0.6, duration: motionMs(120) }}>
                  <input
                    class="ui-input ui-input--dense app-picker-search"
                    type="text"
                    role="combobox"
                    aria-expanded="true"
                    aria-controls={appPickerMenuId}
                    placeholder="Search apps…"
                    bind:value={appPickerQuery}
                    bind:this={appPickerInput}
                    onkeydown={handleAppPickerInputKeydown}
                    autocomplete="off"
                  />
                  <div id={appPickerMenuId} role="listbox" aria-label="Apps" class="app-picker-list">
                    {#if appPickerMatches.length === 0}
                      <span class="picker-empty">{installedApps.length === 0 ? 'No apps discovered yet.' : 'No matching apps.'}</span>
                    {:else}
                      {#each appPickerMatches as app (app.exe)}
                        <button
                          class="ui-dropdown-option"
                          type="button"
                          role="option"
                          aria-selected="false"
                          onclick={() => void assignApp(app)}
                          onkeydown={(event) => handleListboxOptionKeydown(event, appPickerMenuId, () => appPickerInput?.focus())}
                        >
                          <AppIcon exe={app.exe} label={app.name} size={16} />
                          <span>{cleanAppName(app.name || app.exe)}</span>
                          <span class="app-exe">{app.exe}</span>
                        </button>
                      {/each}
                    {/if}
                  </div>
                </div>
              {/if}
              <button class="btn-ghost btn-compact app-picker-trigger" type="button" bind:this={websitePickerTrigger} onclick={() => void toggleWebsitePicker()} aria-expanded={websitePickerOpen} aria-haspopup="dialog">
                Add website
              </button>
              {#if websitePickerOpen}
                <div class="app-picker ui-dropdown-menu website-picker" role="presentation" onpointerdown={(event) => event.stopPropagation()} in:fly={{ y: motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.fast) }} out:fly={{ y: motionPx(MOTION_PX.nudge) * 0.6, duration: motionMs(120) }}>
                  <input
                    class="ui-input ui-input--dense app-picker-search"
                    type="text"
                    placeholder="mail.google.com"
                    bind:value={websiteInput}
                    bind:this={websiteInputEl}
                    disabled={checkingDomain}
                    oninput={() => websiteError = ''}
                    onkeydown={handleWebsiteInputKeydown}
                    autocomplete="off"
                  />
                  {#if websiteInput.trim() && !websiteError}
                    <p class="domain-preview" class:is-valid={websiteValid}>
                      {#if websiteValid}
                        <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
                        Will match <strong>{websitePreview}</strong>
                      {:else}
                        That doesn't look like a domain yet
                      {/if}
                    </p>
                  {/if}
                  {#if websiteError}<p class="save-error">{websiteError}</p>{/if}
                  <button class="btn-primary btn-compact" type="button" onclick={() => void assignWebsite()} disabled={!websiteValid || checkingDomain}>{checkingDomain ? 'Checking…' : 'Add'}</button>
                </div>
              {/if}
            </div>
          {/if}
        </div>

        {#if contextStats && contextStats.dictations > 0}
          <div class="context-stats" in:fly={{ y: motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.fast) }}>
            <span class="context-stat"><strong>{contextStats.words.toLocaleString()}</strong> {contextStats.words === 1 ? 'word' : 'words'}</span>
            <span class="context-stat-sep" aria-hidden="true">·</span>
            <span class="context-stat"><strong>{contextStats.dictations.toLocaleString()}</strong> {contextStats.dictations === 1 ? 'dictation' : 'dictations'}</span>
            {#if contextStats.last_used_at}
              <span class="context-stat-sep" aria-hidden="true">·</span>
              <span class="context-stat">last used {fmtDictionaryDate(contextStats.last_used_at)}</span>
            {/if}
          </div>
        {/if}

        {#if !selectedContext.is_everywhere}
          <div class="target-strip">
            <span class="target-label">Apps &amp; sites</span>
            {#each selectedTargets as target (target.id)}
              <span
                class="target-chip"
                animate:flip={{ duration: motionMs(220), easing: expoOut }}
                in:fly={{ y: motionPx(MOTION_PX.nudge), duration: recentlyAddedChips.has(normalizeExe(target.executable)) ? motionMs(220) : 0, easing: expoOut }}
              >
                <AppIcon exe={target.executable} label={appLabel(target.executable)} size={16} />
                {appLabel(target.executable)}
                <button type="button" aria-label={`Remove ${appLabel(target.executable)}`} onclick={() => removeApp(target)}>
                  <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg>
                </button>
              </span>
            {/each}
            {#each selectedWebsites as site (site.id)}
              <span
                class="target-chip"
                animate:flip={{ duration: motionMs(220), easing: expoOut }}
                in:fly={{ y: motionPx(MOTION_PX.nudge), duration: recentlyAddedChips.has(site.domain) ? motionMs(220) : 0, easing: expoOut }}
              >
                <SiteIcon domain={site.domain} size={16} />
                {site.domain}
                <button type="button" aria-label={`Remove ${site.domain}`} onclick={() => removeWebsite(site)}>
                  <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg>
                </button>
              </span>
            {/each}
            {#if selectedTargets.length === 0 && selectedWebsites.length === 0}<span class="target-empty">No apps or sites assigned yet</span>{/if}
          </div>
        {/if}

        <div class="tabs" role="tablist" tabindex="-1" bind:this={tablistEl} onkeydown={handleTablistKeydown}>
          {#each CONTEXT_TABS as t}
            <button
              class="tab ui-focus-ring"
              class:active={tab === t.id}
              role="tab"
              id="context-tab-{t.id}"
              tabindex={tab === t.id ? 0 : -1}
              aria-selected={tab === t.id}
              aria-controls="context-panel-{t.id}"
              onclick={() => selectTab(t.id)}
            >
              {t.label}
              <span class="tab-count">{t.id === 'vocabulary' ? filteredDictionary.length : filteredSnippets.length}</span>
              {#if tab === t.id}
                <div class="active-bar" in:receive={{key: 'context-tab'}} out:send={{key: 'context-tab'}}></div>
              {/if}
            </button>
          {/each}
        </div>

        <div class="library-toolbar">
          <div class="library-search">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="11" cy="11" r="7"/><path d="m21 21-4.35-4.35"/></svg>
            <input class="ui-input ui-input--dense" type="text" placeholder={tab === 'vocabulary' ? 'Search vocabulary…' : 'Search snippets…'} bind:value={search} aria-label="Search this context group" />
            {#if search}<button class="clear-btn ui-focus-ring" type="button" aria-label="Clear search" onclick={() => search = ''}>×</button>{/if}
          </div>
          <button class="btn-primary btn-compact" type="button" onclick={tab === 'vocabulary' ? openAddDictionary : openAddSnippet}>
            {tab === 'vocabulary' ? '+ Term' : '+ Snippet'}
          </button>
        </div>

        {#if loading && dictionary.length === 0 && snippets.length === 0}
          <div class="context-loading" role="status">Loading this context group…</div>
        {:else}
          {#key tab}
            <div
              class="tab-panel"
              class:is-refreshing={loading}
              role="tabpanel"
              id="context-panel-{tab}"
              aria-labelledby="context-tab-{tab}"
              in:pageSwap={{ axis: 'x', distance: tabDir * motionPx(MOTION_PX.page), duration: motionMs(MOTION_MS.base) }}
            >
              {#if tab === 'vocabulary'}
                {#if filteredDictionary.length === 0}
                  <div class="section-empty">
                    <div class="section-empty-mark" aria-hidden="true">
                      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"/><path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2Z"/></svg>
                    </div>
                    <div>
                      <p>{search ? 'No matching terms' : 'No vocabulary in this context group'}</p>
                      <span>{search ? 'Try a different search.' : 'Add terms Verenu should recognize here.'}</span>
                    </div>
                    {#if search}
                      <button class="btn-ghost btn-compact" type="button" onclick={() => search = ''}>Clear search</button>
                    {:else}
                      <button class="btn-primary btn-compact" type="button" onclick={openAddDictionary}>Add term</button>
                    {/if}
                  </div>
                {:else}
                  <div class="item-list">
                    {#each filteredDictionary as e (e.id)}
                      <!-- Mouse convenience only — the kebab button inside is the
                           keyboard-accessible equivalent for this same action. -->
                      <!-- svelte-ignore a11y_click_events_have_key_events -->
                      <!-- svelte-ignore a11y_no_static_element_interactions -->
                      <div class="item-row" onclick={(event) => handleItemRowClick('dictionary', e.id, event)}>
                        <span class="item-main">
                          <span class="item-term">{e.term}</span>
                          {#if e.auto_learned}
                            <svg class="item-auto-star" width="11" height="11" viewBox="0 0 24 24" fill="currentColor" aria-label="Auto-learned"><title>Added automatically by Auto-learn</title><path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z"/></svg>
                          {/if}
                          {#if e.mistake}
                            <span class="item-often">often:</span>
                            <span class="item-detail">"{e.mistake}"</span>
                          {/if}
                        </span>
                        <span class="item-meta">
                          {#if e.correction_count > 0}<span>{e.correction_count} {e.correction_count === 1 ? 'correction' : 'corrections'}</span>{/if}
                          {#if e.auto_learned}<span>{confidenceLabel(e.confidence_tier)}</span>{/if}
                          <span>{fmtDictionaryDate(e.created_at)}</span>
                        </span>
                        <button
                          type="button"
                          class="item-kebab"
                          aria-label={`More actions for ${e.term}`}
                          aria-haspopup="menu"
                          aria-expanded={openRowMenu?.kind === 'dictionary' && openRowMenu.id === e.id}
                          onclick={(event) => toggleRowMenu('dictionary', e.id, event.currentTarget)}
                        >
                          <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><circle cx="12" cy="5" r="1.8"/><circle cx="12" cy="12" r="1.8"/><circle cx="12" cy="19" r="1.8"/></svg>
                        </button>
                      </div>
                    {/each}
                  </div>
                {/if}
              {:else}
                {#if filteredSnippets.length === 0}
                  <div class="section-empty">
                    <div class="section-empty-mark" aria-hidden="true">
                      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2Z"/><path d="M14 2v6h6M8 13h8M8 17h5"/></svg>
                    </div>
                    <div>
                      <p>{search ? 'No matching snippets' : 'No snippets in this context group'}</p>
                      <span>{search ? 'Try a different search.' : 'Add a spoken trigger for repeated text.'}</span>
                    </div>
                    {#if search}
                      <button class="btn-ghost btn-compact" type="button" onclick={() => search = ''}>Clear search</button>
                    {:else}
                      <button class="btn-primary btn-compact" type="button" onclick={openAddSnippet}>Add snippet</button>
                    {/if}
                  </div>
                {:else}
                  <div class="item-list">
                    {#each filteredSnippets as s (s.id)}
                      <!-- svelte-ignore a11y_click_events_have_key_events -->
                      <!-- svelte-ignore a11y_no_static_element_interactions -->
                      <div class="item-row" onclick={(event) => handleItemRowClick('snippet', s.id, event)}>
                        <span class="item-main">
                          <span class="item-term">{s.trigger}</span>
                          <span class="item-arrow" aria-hidden="true">→</span>
                          <span class="item-detail">{s.expansion}</span>
                        </span>
                        <span class="item-meta">
                          <span>{s.use_count} {s.use_count === 1 ? 'use' : 'uses'}</span>
                          <span>{fmtSnippetDate(s.created_at)}</span>
                        </span>
                        <button
                          type="button"
                          class="item-kebab"
                          aria-label={`More actions for ${s.trigger}`}
                          aria-haspopup="menu"
                          aria-expanded={openRowMenu?.kind === 'snippet' && openRowMenu.id === s.id}
                          onclick={(event) => toggleRowMenu('snippet', s.id, event.currentTarget)}
                        >
                          <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><circle cx="12" cy="5" r="1.8"/><circle cx="12" cy="12" r="1.8"/><circle cx="12" cy="19" r="1.8"/></svg>
                        </button>
                      </div>
                    {/each}
                  </div>
                {/if}
              {/if}
            </div>
          {/key}
        {/if}
        </div>
      {:else}
        <div class="context-empty" in:pageSwap={{ axis: 'y', distance: motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.base) }}>Create a context group to get started.</div>
      {/if}
      {/key}
    </main>
  </div>
</div>

{#if modalColorPickerOpen && modalColorPickerPos}
  <div
    class="ui-dropdown-menu color-picker-fixed"
    role="menu"
    tabindex="-1"
    style="top: {modalColorPickerPos.top}px; left: {modalColorPickerPos.left}px;"
    onpointerdown={(event) => event.stopPropagation()}
    in:fly={{ y: motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.fast) }} out:fly={{ y: motionPx(MOTION_PX.nudge) * 0.6, duration: motionMs(120) }}
  >
    <div class="color-swatch-row">
      {#each CONTEXT_COLOR_CHOICES as choice (choice.id)}
        <button
          type="button"
          class="color-swatch"
          class:is-selected={modalColor === choice.value}
          style="background: {choice.value};"
          aria-label={choice.label}
          title={choice.label}
          onclick={() => pickModalColor(choice.value)}
        ></button>
      {/each}
      <button
        type="button"
        class="color-swatch color-swatch-none"
        class:is-selected={!modalColor}
        aria-label="Default"
        title="Default"
        onclick={() => pickModalColor(null)}
      >
        <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg>
      </button>
    </div>
  </div>
{/if}

{#if openRowMenu && rowMenuPos}
  <div
    class="ui-dropdown-menu row-menu-fixed"
    role="menu"
    tabindex="-1"
    style="top: {rowMenuPos.top}px; right: {rowMenuPos.right}px;"
    onpointerdown={(event) => event.stopPropagation()}
    in:fly={{ y: motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.fast) }} out:fly={{ y: motionPx(MOTION_PX.nudge) * 0.6, duration: motionMs(120) }}
  >
    {#if !rowMenuShowMove}
      <button class="ui-dropdown-option" type="button" role="menuitem" onclick={() => {
        if (openRowMenu?.kind === 'dictionary') {
          const entry = dictionary.find((d) => d.id === openRowMenu!.id);
          if (entry) editDictionary(entry);
        } else if (openRowMenu?.kind === 'snippet') {
          const snippet = snippets.find((sn) => sn.id === openRowMenu!.id);
          if (snippet) editSnippet(snippet);
        }
      }}>Edit</button>
      {#if contexts.length > 1}
        <button class="ui-dropdown-option" type="button" role="menuitem" onclick={() => rowMenuShowMove = true}>Move to…</button>
      {/if}
      <button
        class="ui-dropdown-option row-menu-delete"
        class:is-armed={rowMenuDeleteArmed}
        type="button"
        role="menuitem"
        onclick={() => openRowMenu && void confirmDeleteRow(openRowMenu.kind, openRowMenu.id)}
      >
        {rowMenuDeleteArmed ? 'Confirm delete' : 'Delete'}
      </button>
    {:else}
      <button class="ui-dropdown-option row-menu-back" type="button" role="menuitem" onclick={() => rowMenuShowMove = false}>
        <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="m15 18-6-6 6-6"/></svg>
        Back
      </button>
      <div class="row-menu-move-list" role="group" aria-label="Move to context group">
        {#each contexts.filter((c) => c.id !== selectedContextId) as target (target.id)}
          <button
            class="ui-dropdown-option"
            type="button"
            role="menuitem"
            onclick={() => openRowMenu && void moveRowItem(openRowMenu.kind, openRowMenu.id, target.id)}
          >
            {target.name}
          </button>
        {/each}
      </div>
    {/if}
  </div>
{/if}

{#if modal === 'dictionary'}
  <DictionaryModal
    mode={selectedDictionary ? 'edit' : 'add'}
    entry={selectedDictionary ?? undefined}
    onClose={() => modal = null}
    onSaved={handleDictionarySaved}
    onGoToSnippets={() => modal = 'snippet'}
  />
{:else if modal === 'snippet'}
  <SnippetModal
    mode={selectedSnippet ? 'edit' : 'add'}
    snippet={selectedSnippet ?? undefined}
    onClose={() => modal = null}
    onSaved={handleSnippetSaved}
  />
{:else if modal === 'context'}
  <div class="ui-modal-backdrop" in:modalBackdrop={{ duration: 180 }} out:modalBackdrop={{ duration: 160 }}></div>
  <div
    class="modal-card ui-modal-card context-modal"
    use:modalFocusTrap={{ active: true, initialFocus: () => contextInput }}
    role="dialog"
    aria-modal="true"
    aria-labelledby="context-modal-title"
    tabindex="-1"
    in:modalCard={{ duration: 220, distance: motionPx(MOTION_PX.panel), scaleFrom: 0.97 }}
    out:modalCard={{ duration: 160, distance: motionPx(MOTION_PX.nudge), scaleFrom: 0.985 }}
  >
    <div class="ui-modal-head">
      <h2 id="context-modal-title" class="ui-modal-title">{contextModalMode === 'create' ? 'New context group' : 'Edit context group'}</h2>
      <button class="icon-btn" type="button" onclick={closeModal} aria-label="Close">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg>
      </button>
    </div>
    <div class="ui-modal-body">
      <div class="field-label-row">
        <label class="field-label" for="context-name">Context group name</label>
        <span class="char-counter" class:is-limit={contextName.length >= CONTEXT_NAME_MAX_LENGTH}>{contextName.length}/{CONTEXT_NAME_MAX_LENGTH}</span>
      </div>
      <input id="context-name" class="ui-input" bind:this={contextInput} bind:value={contextName} maxlength={CONTEXT_NAME_MAX_LENGTH} placeholder="e.g. Development, Writing, Work" autocomplete="off" />

      <span class="field-label">Icon</span>
      <div class="icon-grid" role="radiogroup" aria-label="Context group icon">
        {#each CONTEXT_ICON_CHOICES as iconKey}
          <button
            type="button"
            class="icon-choice"
            class:is-selected={modalIcon === iconKey}
            style={modalIcon === iconKey && modalColor ? `color: ${modalColor}; border-color: ${modalColor};` : ''}
            role="radio"
            aria-checked={modalIcon === iconKey}
            aria-label={iconKey}
            onclick={() => modalIcon = modalIcon === iconKey ? null : iconKey}
            oncontextmenu={openModalColorPicker}
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">{@html icons[iconKey]}</svg>
          </button>
        {/each}
      </div>
      <p class="field-hint">Right-click an icon to set its color.</p>

      <div class="field-row">
        <div class="field-col">
          <span class="field-label">Tone</span>
          <button
            type="button"
            bind:this={toneTrigger}
            class="ui-dropdown-trigger ui-dropdown-trigger--compact"
            aria-haspopup="listbox"
            aria-expanded={openFieldMenu === 'tone'}
            onclick={() => toggleFieldMenu('tone', toneTrigger)}
          >
            {modalTone ? getProfileLabel(modalTone) : 'Use default'}
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>
          </button>
        </div>
        <div class="field-col">
          <span class="field-label">Cleanup</span>
          <button
            type="button"
            bind:this={cleanupTrigger}
            class="ui-dropdown-trigger ui-dropdown-trigger--compact"
            aria-haspopup="listbox"
            aria-expanded={openFieldMenu === 'cleanup'}
            onclick={() => toggleFieldMenu('cleanup', cleanupTrigger)}
          >
            {modalCleanupIntensity ? getCleanupIntensityLabel(modalCleanupIntensity) : 'Use default'}
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>
          </button>
        </div>
      </div>

      <div class="field-label-row">
        <label class="field-label" for="context-custom-instructions">Custom instructions</label>
        <span class="char-counter" class:is-limit={modalCustomInstructions.length >= CONTEXT_CUSTOM_INSTRUCTIONS_MAX_LENGTH}>{modalCustomInstructions.length}/{CONTEXT_CUSTOM_INSTRUCTIONS_MAX_LENGTH}</span>
      </div>
      <textarea
        id="context-custom-instructions"
        class="ui-input custom-instructions-input scrollbar-standard"
        placeholder="e.g. Always write dates as DD/MM/YYYY."
        bind:value={modalCustomInstructions}
        maxlength={CONTEXT_CUSTOM_INSTRUCTIONS_MAX_LENGTH}
        rows="3"
        spellcheck="false"
      ></textarea>
      <p class="field-hint">Sent directly to the cleanup model whenever this context group is active.</p>

      <div class="advanced-disclosure">
        <button
          class="advanced-trigger"
          type="button"
          aria-expanded={modalAdvancedOpen}
          aria-controls="context-advanced-settings"
          onclick={() => modalAdvancedOpen = !modalAdvancedOpen}
        >
          <span>
            <span class="advanced-trigger-label">Advanced</span>
            <span class="advanced-trigger-hint">Context-specific behavior</span>
          </span>
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>
        </button>
        {#if modalAdvancedOpen}
          <div id="context-advanced-settings" class="advanced-content" transition:slide={{ duration: motionMs(MOTION_MS.base), easing: cubicOut }}>
            <div class="advanced-setting-row">
              <div>
                <span class="advanced-setting-label">Disable smart formatting</span>
                <p>Skips automatic spacing and capitalization in this context.</p>
              </div>
              <Toggle checked={modalContextualFormattingDisabled} onchange={(value) => modalContextualFormattingDisabled = value} label="Disable smart formatting for this context" />
            </div>
          </div>
        {/if}
      </div>

      {#if contextModalMode === 'create'}
        <label class="field-label" for="context-app">Attach apps (optional)</label>
        {#if modalApps.length > 0}
          <div class="modal-chip-row">
            {#each modalApps as app (app.exe)}
              <span
                class="target-chip"
                animate:flip={{ duration: motionMs(220), easing: expoOut }}
                in:fly={{ y: motionPx(MOTION_PX.nudge), duration: motionMs(220), easing: expoOut }}
              >
                <AppIcon exe={app.exe} label={app.name} size={16} />
                {cleanAppName(app.name || app.exe)}
                <button type="button" aria-label={`Remove ${app.name}`} onclick={() => removeModalApp(app.exe)}>
                  <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg>
                </button>
              </span>
            {/each}
          </div>
        {/if}
        <input
          id="context-app"
          class="ui-input"
          type="text"
          placeholder="Search installed apps…"
          bind:value={modalAppQuery}
          bind:this={modalAppInputEl}
          autocomplete="off"
          role="combobox"
          aria-expanded={modalAppQuery.trim().length > 0}
          aria-controls="context-app-matches"
          oninput={updateModalAppMatchPos}
          onfocus={updateModalAppMatchPos}
          onkeydown={handleModalAppKeydown}
        />

        <label class="field-label" for="context-website">Attach websites (optional)</label>
        {#if modalWebsites.length > 0}
          <div class="modal-chip-row">
            {#each modalWebsites as domain (domain)}
              <span
                class="target-chip"
                animate:flip={{ duration: motionMs(220), easing: expoOut }}
                in:fly={{ y: motionPx(MOTION_PX.nudge), duration: motionMs(220), easing: expoOut }}
              >
                <SiteIcon {domain} size={16} />
                {domain}
                <button type="button" aria-label={`Remove ${domain}`} onclick={() => removeModalWebsite(domain)}>
                  <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg>
                </button>
              </span>
            {/each}
          </div>
        {/if}
        <div class="website-input-row">
          <input id="context-website" class="ui-input" type="text" placeholder="mail.google.com" bind:value={modalWebsiteInput} autocomplete="off" disabled={checkingDomain} oninput={() => modalWebsiteError = ''} onkeydown={(event) => { if (event.key === 'Enter') { event.preventDefault(); void addModalWebsite(); } }} />
          <button class="btn-ghost btn-compact website-add-btn" class:is-visible={modalWebsiteValid} type="button" onclick={() => void addModalWebsite()} disabled={!modalWebsiteValid || checkingDomain} tabindex={modalWebsiteValid ? 0 : -1}>{checkingDomain ? 'Checking…' : 'Add'}</button>
        </div>
        {#if modalWebsiteError}
          <p class="save-error">{modalWebsiteError}</p>
        {:else if modalWebsiteInput.trim()}
          <p class="domain-preview" class:is-valid={modalWebsiteValid}>
            {#if modalWebsiteValid}
              <svg width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"/></svg>
              Will match <strong>{modalWebsitePreview}</strong>
            {:else}
              That doesn't look like a domain yet — try something like mail.google.com
            {/if}
          </p>
        {/if}
        <p class="field-hint">Dictate in one of these apps or sites to use this context group automatically. You can add more later too.</p>
      {:else}
        <p class="field-hint">Manage this context group's apps and websites from the header above.</p>
      {/if}
    </div>
    <div class="ui-modal-foot">
      {#if contextError}<p class="save-error">{contextError}</p>{/if}
      <div class="ui-modal-actions">
        <button class="btn-ghost" type="button" onclick={closeModal}>Cancel</button>
        <button class="btn-primary" type="button" onclick={() => void saveContext()} disabled={savingContext}>{savingContext ? 'Saving…' : contextModalMode === 'create' ? 'Create context group' : 'Save changes'}</button>
      </div>
    </div>
  </div>
  {#if modalAppQuery.trim() && modalAppMatchPos}
    <div
      id="context-app-matches"
      class="ui-dropdown-menu modal-app-matches"
      role="listbox"
      tabindex="-1"
      aria-label="Matching apps"
      style="top: {modalAppMatchPos.top}px; left: {modalAppMatchPos.left}px; width: {modalAppMatchPos.width}px;"
      onpointerdown={(event) => event.stopPropagation()}
      in:fly={{ y: motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.fast) }} out:fly={{ y: motionPx(MOTION_PX.nudge) * 0.6, duration: motionMs(120) }}
    >
      {#if modalAppMatches.length === 0}
        <span class="picker-empty">No matching apps.</span>
      {:else}
        {#each modalAppMatches as app, index (app.exe)}
          <button
            class="ui-dropdown-option"
            class:is-active={index === modalAppHighlight}
            type="button"
            role="option"
            aria-selected={index === modalAppHighlight}
            onclick={() => pickModalApp(app)}
            onpointerenter={() => modalAppHighlight = index}
          >
            <AppIcon exe={app.exe} label={app.name} size={16} />
            <span>{cleanAppName(app.name || app.exe)}</span>
            <span class="app-exe">{app.exe}</span>
          </button>
        {/each}
      {/if}
    </div>
  {/if}
  {#if openFieldMenu && fieldMenuPos}
    <div
      class="ui-dropdown-menu field-menu-fixed"
      role="listbox"
      tabindex="-1"
      aria-label={openFieldMenu === 'tone' ? 'Tone' : 'Cleanup intensity'}
      style="top: {fieldMenuPos.top}px; left: {fieldMenuPos.left}px; min-width: {fieldMenuPos.width}px;"
      onpointerdown={(event) => event.stopPropagation()}
      in:fly={{ y: motionPx(MOTION_PX.nudge), duration: motionMs(MOTION_MS.fast) }} out:fly={{ y: motionPx(MOTION_PX.nudge) * 0.6, duration: motionMs(120) }}
    >
      {#if openFieldMenu === 'tone'}
        <button class="ui-dropdown-option" type="button" role="option" aria-selected={modalTone === null} onclick={() => { modalTone = null; closeFieldMenu(); }}>Use default</button>
        {#each profileOptions as profile}
          <button class="ui-dropdown-option" type="button" role="option" aria-selected={modalTone === profile.id} onclick={() => { modalTone = profile.id; closeFieldMenu(); }}>{profile.label}</button>
        {/each}
      {:else}
        <button class="ui-dropdown-option" type="button" role="option" aria-selected={modalCleanupIntensity === null} onclick={() => { modalCleanupIntensity = null; closeFieldMenu(); }}>Use default</button>
        {#each cleanupIntensityOptions as choice}
          <button class="ui-dropdown-option" type="button" role="option" aria-selected={modalCleanupIntensity === choice.id} onclick={() => { modalCleanupIntensity = choice.id; closeFieldMenu(); }}>{choice.label}</button>
        {/each}
      {/if}
    </div>
  {/if}
{/if}

<style>
  .content-inner {
    width: min(100%, var(--page-max));
    margin-inline: auto;
    padding: var(--page-pad-y) var(--page-pad-x) 36px;
    min-width: 0;
  }
  .load-warning {
    margin: 0 0 16px;
    padding: 8px 10px;
    border-radius: var(--r-sm);
    border: 1px solid var(--danger-line);
    background: var(--danger-bg);
    color: var(--danger);
    font-size: 11.5px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }
  .contexts-page { min-width: 0; }
  .contexts-page:not(.modal-open) ~ :global(.modal-card),
  .contexts-page:not(.modal-open) ~ :global(.ui-modal-backdrop) { pointer-events: none; }
  .contexts-shell { min-width: 0; }
  /* One cell, so the outgoing and incoming context panes cross-fade in place. */
  .context-main { display: grid; }
  .context-content { grid-area: 1 / 1; min-width: 0; }

  /*
   * A light cascade behind the pane swap: header, stats, targets, tabs and the
   * list each settle a beat apart, which is what makes switching context feel
   * like something happened rather than a straight cross-fade. Keyed remount
   * replays it on every switch.
   */
  .context-content > :global(*) {
    animation: context-rise var(--ctx-rise-ms) cubic-bezier(0.33, 1, 0.68, 1) both;
  }
  .context-content { --ctx-rise-ms: 300ms; }
  .context-content > :global(:nth-child(1)) { animation-delay: 0ms; }
  .context-content > :global(:nth-child(2)) { animation-delay: 35ms; }
  .context-content > :global(:nth-child(3)) { animation-delay: 70ms; }
  .context-content > :global(:nth-child(4)) { animation-delay: 105ms; }
  .context-content > :global(:nth-child(n + 5)) { animation-delay: 140ms; }

  @keyframes context-rise {
    from { opacity: 0; transform: translate3d(0, 6px, 0); }
    to   { opacity: 1; transform: none; }
  }

  @media (prefers-reduced-motion: reduce) {
    .context-content { --ctx-rise-ms: 1ms; }
    .context-content > :global(*) { animation-delay: 0ms !important; }
  }
  .context-header, .target-strip, .library-toolbar { display: flex; align-items: center; }
  .context-main { min-width: 0; position: relative; }
  .context-stats {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 7px;
    margin: -8px 0 16px;
    font-size: 11.5px;
    color: var(--ink-mute);
  }
  .context-stats strong { color: var(--ink-soft); font-weight: 550; font-variant-numeric: tabular-nums; }
  .context-stat-sep { color: var(--ink-faint); }
  .context-header { justify-content: space-between; gap: 16px; margin-bottom: 16px; }
  .context-actions { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; justify-content: flex-end; position: relative; }
  .context-header h2 { font-family: var(--serif); font-size: 26px; font-weight: 500; letter-spacing: -.02em; line-height: 1.1; margin: 4px 0 4px; color: var(--ink); }
  .context-header p { color: var(--ink-mute); font-size: 12px; margin: 0; line-height: 1.45; }
  .target-strip { flex-wrap: wrap; gap: 6px; padding: 9px 0 14px; border-top: 1px solid var(--line-soft); }
  .target-label { color: var(--ink-faint); font-family: var(--mono); font-size: 10px; letter-spacing: .08em; text-transform: uppercase; margin-right: 3px; }
  .target-chip { display: inline-flex; align-items: center; gap: 5px; padding: 4px 6px 4px 4px; border: 1px solid var(--line); border-radius: 8px; background: var(--bg-elev); color: var(--ink-soft); font-size: 11px; }
  .target-chip button { display: grid; place-items: center; border: 0; background: transparent; color: var(--ink-faint); padding: 2px; cursor: pointer; border-radius: 4px; }
  .target-chip button:hover { color: var(--danger); background: var(--danger-bg); }
  .target-empty { color: var(--ink-faint); font-size: 11px; font-style: italic; }
  .app-picker { position: absolute; z-index: 5; top: 100%; right: 0; margin-top: 6px; width: min(320px, calc(100vw - 64px)); display: flex; flex-direction: column; gap: 6px; padding: 8px; }
  .app-picker-search { width: 100%; }
  .app-picker-list { max-height: 240px; overflow: auto; display: flex; flex-direction: column; gap: 1px; }
  .website-picker { width: min(280px, calc(100vw - 64px)); }
  .ui-dropdown-option { display: flex; align-items: center; gap: 8px; width: 100%; text-align: left; }
  .app-exe { color: var(--ink-faint); font-family: var(--mono); font-size: 10px; margin-left: auto; }
  .picker-empty { display: block; padding: 8px 4px; color: var(--ink-mute); font-size: 11px; }
  .modal-app-matches { position: fixed; z-index: 60; max-height: 220px; overflow-y: auto; display: flex; flex-direction: column; gap: 1px; padding: 4px; }
  .modal-chip-row { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 4px; }
  .website-input-row { display: flex; align-items: center; }
  .website-input-row .ui-input { flex: 1; min-width: 0; }
  .website-add-btn {
    flex: 0 0 auto;
    max-width: 0;
    margin-left: 0;
    padding-left: 0;
    padding-right: 0;
    opacity: 0;
    overflow: hidden;
    white-space: nowrap;
    pointer-events: none;
    transition: max-width .22s var(--ui-ease-out, ease), opacity .16s ease, margin-left .22s var(--ui-ease-out, ease), padding .22s var(--ui-ease-out, ease);
  }
  .website-add-btn.is-visible {
    max-width: 90px;
    margin-left: 6px;
    padding-left: 14px;
    padding-right: 14px;
    opacity: 1;
    pointer-events: auto;
  }
  .icon-grid { display: grid; grid-template-columns: repeat(6, 1fr); gap: 6px; margin: 4px 0 2px; }
  .icon-choice { display: grid; place-items: center; height: 32px; border-radius: 8px; border: 1px solid var(--line); background: var(--bg-elev); color: var(--ink-mute); cursor: pointer; }
  .icon-choice:hover { color: var(--ink-soft); background: var(--control-hover); }
  .icon-choice.is-selected { border-color: var(--accent); color: var(--accent-ink); background: var(--accent-soft); }
  .field-row { display: flex; gap: 10px; }
  .field-col { flex: 1; min-width: 0; }
  .field-col .ui-dropdown-trigger { width: 100%; display: flex; align-items: center; justify-content: space-between; gap: 6px; cursor: pointer; }
  .field-menu-fixed { position: fixed; right: auto; top: 0; left: 0; z-index: 60; }
  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 20px;
    border-bottom: 1px solid var(--line);
    margin-bottom: 14px;
  }
  .tab {
    padding: 0 0 9px;
    font-size: 13px;
    color: var(--ink-mute);
    border: 0;
    background: transparent;
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
    position: relative;
  }
  .tab:hover { color: var(--ink-soft); }
  .tab.active { color: var(--ink); font-weight: 500; }
  .active-bar { position: absolute; bottom: -1px; left: 0; right: 0; height: 1px; background: var(--ink); }
  .tab-count { font-family: var(--mono); font-size: 10px; color: var(--ink-faint); background: var(--bg-elev); border: 1px solid var(--line); border-radius: 99px; padding: 1px 6px; }
  .tab.active .tab-count { color: var(--ink-mute); }
  .tab-panel { min-width: 0; transition: opacity .18s ease; }
  .tab-panel.is-refreshing { opacity: .5; }
  .library-toolbar { gap: 7px; flex-wrap: wrap; margin: 4px 0 16px; }
  .library-search { flex: 1 1 190px; min-width: 150px; height: 32px; padding: 0 9px; display: flex; align-items: center; gap: 7px; background: var(--bg-elev); border: 1px solid var(--line); border-radius: var(--r-sm); color: var(--ink-mute); }
  .library-search .ui-input { min-width: 0; flex: 1; border-color: transparent; background: transparent; }
  .library-search .ui-input:focus-visible { border-color: var(--line-strong); outline: none; }
  .clear-btn { border: 0; background: transparent; color: var(--ink-mute); cursor: pointer; font-size: 16px; line-height: 1; padding: 0 2px; }
  .section-empty { min-height: 112px; display: flex; align-items: center; gap: 12px; padding: 18px 16px; border: 1px dashed var(--line); border-radius: var(--r-md); background: color-mix(in srgb, var(--bg-elev) 62%, transparent); }
  .section-empty-mark { width: 30px; height: 30px; border-radius: 8px; display: grid; place-items: center; flex: 0 0 30px; background: var(--paper); color: var(--ink-faint); }
  .section-empty p { margin: 0 0 2px; color: var(--ink-soft); font-size: 12px; font-weight: 500; }
  .section-empty span { display: block; color: var(--ink-mute); font-size: 11px; line-height: 1.4; }
  .section-empty .btn-compact { margin-left: auto; flex: 0 0 auto; }
  .item-list { border: 1px solid var(--line); border-radius: var(--r-md); overflow: hidden; background: var(--bg-elev); }
  .item-row { display: grid; grid-template-columns: 1fr auto auto; align-items: center; gap: 14px; padding: 11px 8px 11px 14px; border-bottom: 1px solid var(--line); cursor: pointer; }
  .item-row:last-child { border-bottom: 0; }
  .item-row:hover { background: var(--control-hover); }
  .item-main { display: flex; align-items: baseline; gap: 6px; min-width: 0; overflow: hidden; }
  .item-term { font-size: 13px; font-weight: 500; color: var(--ink); flex-shrink: 0; }
  .item-auto-star { color: var(--accent); flex-shrink: 0; position: relative; top: -1px; }
  .item-often { font-family: var(--mono); font-size: 10px; text-transform: uppercase; letter-spacing: .08em; color: var(--ink-faint); flex-shrink: 0; }
  .item-arrow { color: var(--ink-faint); flex-shrink: 0; }
  .item-detail { font-size: 12.5px; color: var(--ink-mute); font-style: italic; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .item-meta { display: flex; flex-direction: column; align-items: flex-end; gap: 2px; font-size: 11px; color: var(--ink-faint); white-space: nowrap; }
  .item-kebab { display: grid; place-items: center; width: 28px; height: 28px; border: 0; border-radius: 6px; background: transparent; color: var(--ink-faint); cursor: pointer; flex-shrink: 0; }
  .item-kebab:hover, .item-kebab[aria-expanded="true"] { background: var(--control-active); color: var(--ink-soft); }
  .row-menu-fixed { position: fixed; z-index: 60; min-width: 140px; max-width: 220px; }
  .row-menu-move-list { max-height: 220px; overflow-y: auto; display: flex; flex-direction: column; }
  .row-menu-move-list .ui-dropdown-option { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .row-menu-back { color: var(--ink-mute); }
  /* .ui-dropdown-menu sets right:0 for its normal absolute-positioned usage;
     this popup is positioned with an inline `left`, so right must be reset or
     the browser stretches the box from left all the way to the viewport edge. */
  .color-picker-fixed { position: fixed; right: auto; z-index: 60; padding: 8px; width: max-content; }
  .color-swatch-row { display: flex; align-items: center; gap: 7px; }
  .color-swatch {
    width: 22px;
    height: 22px;
    border-radius: 7px;
    border: 1px solid color-mix(in srgb, currentColor 20%, transparent);
    cursor: pointer;
    padding: 0;
    transition: transform .12s ease;
  }
  .color-swatch:hover { transform: scale(1.12); }
  .color-swatch.is-selected { outline: 2px solid var(--ink); outline-offset: 2px; }
  .color-swatch-none {
    display: grid;
    place-items: center;
    background: var(--bg-elev);
    color: var(--ink-faint);
    border-color: var(--line);
  }
  .row-menu-delete { transition: color .15s ease, background-color .15s ease; }
  .row-menu-delete:hover { color: var(--danger); }
  .row-menu-delete.is-armed { color: var(--danger); font-weight: 500; background: var(--danger-bg); }
  @media (max-width: 720px) {
    .item-row { grid-template-columns: 1fr auto; }
    .item-meta { display: none; }
  }
  .context-loading, .context-empty { padding: 52px 10px; color: var(--ink-mute); font-size: 12px; text-align: center; }
  .context-modal { width: min(460px, calc(100vw - 32px)); }
  .context-modal .ui-modal-head { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  .ui-modal-body { display: flex; flex-direction: column; gap: 5px; }
  .field-label { color: var(--ink-soft); font-size: 11.5px; font-weight: 500; margin-top: 10px; }
  .field-label:first-child { margin-top: 0; }
  .field-label-row { display: flex; align-items: baseline; justify-content: space-between; gap: 8px; margin-top: 10px; }
  .field-label-row:first-child { margin-top: 0; }
  .field-label-row .field-label { margin-top: 0; }
  .char-counter { font-size: 10.5px; color: var(--ink-faint); font-variant-numeric: tabular-nums; flex-shrink: 0; }
  .char-counter.is-limit { color: var(--danger); }
  .custom-instructions-input { resize: vertical; font-size: 12.5px; max-height: 160px; }
  .field-hint { color: var(--ink-mute); font-size: 11px; margin: 3px 0 0; }
  .advanced-disclosure { margin-top: 10px; }
  .advanced-trigger { align-items: center; background: transparent; border: 0; border-radius: var(--r-sm); color: var(--ink-soft); cursor: pointer; display: flex; justify-content: space-between; padding: 7px 8px; text-align: left; transition: background-color 150ms ease, color 150ms ease; width: 100%; }
  .advanced-trigger:hover { background: var(--control-hover); color: var(--ink); }
  .advanced-trigger:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
  .advanced-trigger > span { display: flex; flex-direction: column; gap: 1px; }
  .advanced-trigger-label { font-size: 11.5px; font-weight: 500; }
  .advanced-trigger-hint { color: var(--ink-faint); font-size: 10.5px; font-weight: 400; }
  .advanced-trigger svg { color: var(--ink-faint); flex-shrink: 0; transition: transform 180ms cubic-bezier(.22, 1, .36, 1); }
  .advanced-trigger[aria-expanded='true'] svg { transform: rotate(180deg); }
  .advanced-content { overflow: hidden; padding: 5px 0 1px; }
  .advanced-setting-row { align-items: center; background: var(--bg-elev); border: 1px solid var(--line); border-radius: var(--r-sm); display: flex; gap: 16px; justify-content: space-between; padding: 10px 11px; }
  .advanced-setting-label { color: var(--ink); display: block; font-size: 11.5px; font-weight: 500; }
  .advanced-setting-row p { color: var(--ink-mute); font-size: 10.5px; line-height: 1.35; margin: 2px 0 0; }
  .domain-preview { display: flex; align-items: center; gap: 5px; color: var(--ink-faint); font-size: 11px; margin: 5px 0 0; }
  .domain-preview.is-valid { color: var(--ink-mute); }
  .domain-preview svg { color: var(--accent); flex-shrink: 0; }
  .ui-modal-foot { display: flex; flex-direction: column; gap: 10px; }
  .save-error { color: var(--danger); background: var(--danger-bg); border: 1px solid var(--danger-line); border-radius: var(--r-sm); font-size: 11.5px; margin: 0; padding: 6px 10px; }

  @media (max-width: 900px) {
    .app-picker { position: static; width: 100%; margin: 6px 0 0; }
  }
  @media (max-width: 650px) {
    .context-header { align-items: flex-start; flex-direction: column; }
    .section-empty { align-items: flex-start; flex-wrap: wrap; }
    .section-empty .btn-compact { margin-left: 42px; }
  }
</style>
