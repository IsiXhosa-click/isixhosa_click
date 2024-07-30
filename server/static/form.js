export function warnUnsavedChanges(form) {
    let all = Array.from(form.querySelectorAll("input, select, textarea"));
    console.log(all);
    let changes = false;
    let chosen_submit = false;
    let unsaved_changes = form.getElementsByClassName("unsaved-changes")[0];

    for (let elt of all) {
        let event = "change";
        if (elt instanceof HTMLInputElement && elt.type !== "checkbox" || elt instanceof HTMLTextAreaElement) {
            event = "input";
        }

        let orig = getValue(elt);
        elt.setAttribute("data-orig", orig);

        elt.addEventListener(event, function() {
            console.log("GO:");
            changes = all.some(elt => {
                console.log(elt);
                console.log(getValue(elt));
                console.log(elt.getAttribute("data-orig"));
                console.log(getValue(elt) !== elt.getAttribute("data-orig"));
                return getValue(elt) !== elt.getAttribute("data-orig");
            });
            unsaved_changes.hidden = !changes;
        });
    }

    window.addEventListener("beforeunload", function(evt) {
        if (changes && !chosen_submit) {
            evt.preventDefault();
            evt.returnValue = true;
        }
    });

    /* Make the user confirm first */
    form.getElementsByClassName("confirm_yes")[0].addEventListener("click", function () {
        chosen_submit = true;
        form.submit();
    });

    form.getElementsByClassName("confirm_no")[0].addEventListener("click", function() {
        chosen_submit = false;
        form.querySelector(".confirm.modal").classList.remove("open");
    });

    form.addEventListener("submit", function(evt) {
        form.querySelector(".confirm.modal").classList.add("open");
        evt.preventDefault();
        return false;
    })
}

function getValue(elt) {
    return elt instanceof HTMLInputElement && elt.type === "checkbox" ? elt.checked.toString() : elt.value;
}
