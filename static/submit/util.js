export function addFormData(key, value) {
    let suggestion = document.createElement("select");
    suggestion.name = key;
    suggestion.hidden = true;

    let option = document.createElement("option");
    option.value = value;

    suggestion.add(option);

    return suggestion;
}
