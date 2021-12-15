import { LiveSearch } from "/live_search.js?v=9";

function createContainer() {
    let list = document.createElement("ol");
    list.className = "hits";
    return list;
}

function createItem() {
    return document.createElement("span");
}

function createItemContainer(id, is_suggestion) {
    let container = document.createElement("li");
    container.className = "hit_container";

    let inner;

    if (!is_suggestion) {
        inner = document.createElement("a");
        inner.href = `/word/${id}`;
        inner.rel = "noopener noreferrer";
        inner.target = "_blank"
    } else {
        inner = document.createElement("span");
    }

    inner.className = "hit";

    container.appendChild(inner);
    return [container, inner];
}

export function addDuplicateSearchFor(input_id, this_word_id) {
    let input = document.getElementById(input_id);
    /* noinspection EqualityComparisonWithCoercionJS -- this is done intentionally for string to number eq */
    new LiveSearch(
        input,
        document.querySelector(`#${input_id} + .duplicates_container > .duplicates_popover > .duplicates`),
        createContainer,
        createItem,
        function () {},
        createItemContainer,
        r => r.id != this_word_id &&  /* filter */
            (r.english.toLowerCase() === input.value.toLowerCase() || r.xhosa.toLowerCase() === input.value.toLowerCase()),
        true /* include own suggestions */
    );
}
