export function share(text) {
    if (navigator.share) {
        navigator.share({ text })
            .then(() => console.log("Successful share"))
            .catch((error) => console.log("Error sharing", error));
    } else if (navigator.clipboard.writeText) {
        navigator.clipboard.writeText(text);
    }
}
