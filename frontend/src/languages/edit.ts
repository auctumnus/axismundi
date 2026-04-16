document.addEventListener("DOMContentLoaded", () => {
    const bannerInput = document.getElementById("banner") as HTMLInputElement | null;
    const bannerImage = document.getElementById("banner-img-input") as HTMLImageElement | null;

    const setupImagePreview = (input: HTMLInputElement | null, img: HTMLImageElement | null) => {
        if (!input || !img) return;

        input.addEventListener("change", () => {
        const file = input.files?.[0];
        if (!file) return;

        if (img.src) URL.revokeObjectURL(img.src);
        img.src = URL.createObjectURL(file);
        img.classList.remove("no-img");
        })
    };

    setupImagePreview(bannerInput, bannerImage);
});