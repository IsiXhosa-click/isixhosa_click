export function addFormData(key, value) {
    let suggestion = document.createElement("input");
    suggestion.type = "hidden"
    suggestion.name = key;
    suggestion.hidden = true;
    suggestion.value = value

    return suggestion;
}

/* Do not require shift/ctrl click to select multiple */
export function setupSelectMultiple () {
    for (const select of document.querySelectorAll("select[multiple]")) {
        for (const option of select.querySelectorAll('option')) {
            option.addEventListener('mousedown', evt => {
                const scroll = option.parentElement.scrollTop

                evt.preventDefault()
                option.selected = !option.selected
                select.focus()

                setTimeout(function () {
                    option.parentElement.scrollTop = scroll
                }, 0)

                const form = select.closest('form')
                if (form) {
                    form.dispatchEvent(new Event('change'))
                }

                return false
            })
        }

        select.addEventListener('click', evt => {
            evt.preventDefault()
        })
    }
}
