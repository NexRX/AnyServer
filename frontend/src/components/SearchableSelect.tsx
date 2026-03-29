import {
  type Component,
  createSignal,
  createEffect,
  onCleanup,
  For,
  Show,
} from "solid-js";

export interface SearchableSelectOption {
  value: string;
  label: string;
}

export interface SearchableSelectProps {
  options: SearchableSelectOption[];
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  disabled?: boolean;
  allowEmpty?: boolean;
  emptyLabel?: string;
  maxHeight?: string;
}

const SearchableSelect: Component<SearchableSelectProps> = (props) => {
  const [open, setOpen] = createSignal(false);
  const [search, setSearch] = createSignal("");
  const [highlightIndex, setHighlightIndex] = createSignal(0);

  let rootRef: HTMLDivElement | undefined;
  let searchRef: HTMLInputElement | undefined;
  let listRef: HTMLDivElement | undefined;

  const allOptions = (): SearchableSelectOption[] => {
    const base: SearchableSelectOption[] = props.allowEmpty
      ? [{ value: "", label: props.emptyLabel ?? "— none —" }]
      : [];
    return [...base, ...props.options];
  };

  const filtered = (): SearchableSelectOption[] => {
    const q = search().toLowerCase().trim();
    if (!q) return allOptions();
    return allOptions().filter(
      (o) =>
        o.label.toLowerCase().includes(q) ||
        o.value.toLowerCase().includes(q),
    );
  };

  const selectedLabel = (): string => {
    if (props.value === "" && props.allowEmpty) {
      return props.emptyLabel ?? "— none —";
    }
    const match = props.options.find((o) => o.value === props.value);
    return match ? match.label : props.value || props.placeholder || "Select…";
  };

  const hasRealSelection = (): boolean => {
    if (props.value === "" && props.allowEmpty) return true;
    return props.options.some((o) => o.value === props.value);
  };

  const openDropdown = () => {
    if (props.disabled) return;
    setSearch("");
    setHighlightIndex(0);
    setOpen(true);
  };

  const closeDropdown = () => {
    setOpen(false);
    setSearch("");
  };

  const selectOption = (value: string) => {
    props.onChange(value);
    closeDropdown();
  };

  // Focus the search input when the dropdown opens
  createEffect(() => {
    if (open() && searchRef) {
      // Use requestAnimationFrame to ensure the element is in the DOM
      requestAnimationFrame(() => {
        searchRef?.focus();
      });
    }
  });

  // Scroll the selected option into view when the dropdown opens
  createEffect(() => {
    if (open() && listRef) {
      requestAnimationFrame(() => {
        const selectedEl = listRef?.querySelector(
          '[aria-selected="true"]',
        ) as HTMLElement | null;
        if (selectedEl) {
          selectedEl.scrollIntoView({ block: "nearest" });
        }
      });
    }
  });

  // Click-outside-to-close
  const handleClickOutside = (e: MouseEvent) => {
    if (open() && rootRef && !rootRef.contains(e.target as Node)) {
      closeDropdown();
    }
  };

  if (typeof document !== "undefined") {
    document.addEventListener("mousedown", handleClickOutside);
    onCleanup(() =>
      document.removeEventListener("mousedown", handleClickOutside),
    );
  }

  const handleKeyDown = (e: KeyboardEvent) => {
    if (!open()) {
      if (e.key === "Enter" || e.key === " " || e.key === "ArrowDown") {
        e.preventDefault();
        openDropdown();
      }
      return;
    }

    const items = filtered();

    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        setHighlightIndex((prev) => Math.min(prev + 1, items.length - 1));
        scrollHighlightedIntoView();
        break;
      case "ArrowUp":
        e.preventDefault();
        setHighlightIndex((prev) => Math.max(prev - 1, 0));
        scrollHighlightedIntoView();
        break;
      case "Enter":
        e.preventDefault();
        if (items.length > 0 && highlightIndex() < items.length) {
          selectOption(items[highlightIndex()].value);
        }
        break;
      case "Escape":
        e.preventDefault();
        closeDropdown();
        break;
      case "Tab":
        closeDropdown();
        break;
    }
  };

  const scrollHighlightedIntoView = () => {
    requestAnimationFrame(() => {
      const highlighted = listRef?.querySelector(
        ".searchable-select-option-highlighted",
      ) as HTMLElement | null;
      if (highlighted) {
        highlighted.scrollIntoView({ block: "nearest" });
      }
    });
  };

  // Reset highlight when filter changes
  createEffect(() => {
    // Access search() to track it
    search();
    setHighlightIndex(0);
  });

  const maxHeight = () => props.maxHeight ?? "280px";

  return (
    <div
      class="searchable-select"
      ref={rootRef}
      onKeyDown={handleKeyDown}
    >
      {/* Trigger button */}
      <button
        type="button"
        class="searchable-select-trigger"
        classList={{
          "searchable-select-trigger-open": open(),
          "searchable-select-trigger-placeholder": !hasRealSelection() && !open(),
        }}
        role="combobox"
        aria-expanded={open()}
        aria-haspopup="listbox"
        aria-disabled={props.disabled}
        disabled={props.disabled}
        onClick={() => {
          if (open()) {
            closeDropdown();
          } else {
            openDropdown();
          }
        }}
      >
        <span class="searchable-select-trigger-label">{selectedLabel()}</span>
        <span class="searchable-select-trigger-arrow" aria-hidden="true">
          {open() ? "▲" : "▼"}
        </span>
      </button>

      {/* Dropdown panel */}
      <Show when={open()}>
        <div class="searchable-select-dropdown">
          {/* Search input */}
          <div class="searchable-select-search-wrapper">
            <input
              ref={searchRef}
              type="text"
              class="searchable-select-search"
              value={search()}
              onInput={(e) => setSearch(e.currentTarget.value)}
              placeholder="Search…"
              aria-label="Filter options"
              autocomplete="off"
            />
          </div>

          {/* Options list */}
          <div
            ref={listRef}
            class="searchable-select-options"
            role="listbox"
            style={{ "max-height": maxHeight(), "overflow-y": "auto" }}
          >
            <Show
              when={filtered().length > 0}
              fallback={
                <div class="searchable-select-no-results">No matches</div>
              }
            >
              <For each={filtered()}>
                {(opt, index) => {
                  const isSelected = () => opt.value === props.value;
                  const isHighlighted = () => index() === highlightIndex();

                  return (
                    <div
                      class="searchable-select-option"
                      classList={{
                        "searchable-select-option-selected": isSelected(),
                        "searchable-select-option-highlighted":
                          isHighlighted(),
                      }}
                      role="option"
                      aria-selected={isSelected()}
                      onMouseEnter={() => setHighlightIndex(index())}
                      onMouseDown={(e) => {
                        // Prevent the search input from losing focus before
                        // click registers
                        e.preventDefault();
                      }}
                      onClick={() => selectOption(opt.value)}
                    >
                      {opt.label}
                      <Show when={isSelected()}>
                        <span
                          class="searchable-select-check"
                          aria-hidden="true"
                        >
                          ✓
                        </span>
                      </Show>
                    </div>
                  );
                }}
              </For>
            </Show>
          </div>
        </div>
      </Show>
    </div>
  );
};

export default SearchableSelect;
