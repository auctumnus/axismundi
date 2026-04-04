const estimate = async (word: string, soundChangeSetId: string) => {
    const response = await fetch(`/api/sound-change-sets/${soundChangeSetId}/run`, {
        method: "POST",
        headers: {
            "Content-Type": "application/json",
        },
        body: JSON.stringify({ input_words: [word] }),
    });
    const data = await response.json();
    if (response.ok) {
        return data.outputWords[0];
    }
    throw new Error(data.error || "Failed to estimate IPA");
}

document.addEventListener("DOMContentLoaded", () => {
    const wordInput = document.getElementById("word") as HTMLInputElement | null;
    const ipaInput = document.getElementById("ipa") as HTMLInputElement | null;

    if(!wordInput || !ipaInput) return;

    const estimateButton = document.getElementById("estimate-ipa") as HTMLButtonElement | null;
    if (estimateButton) {
        const setRunning = () => {
            estimateButton.disabled = true;
            estimateButton.textContent = "Estimating...";
        }

        const setIdle = () => {
            estimateButton.disabled = false;
            estimateButton.textContent = "Estimate IPA";
        }

        const setErrored = () => {
            estimateButton.disabled = false;
            estimateButton.textContent = "Error :(";
        }

        const estimatorHint = document.getElementById("ipa-estimator-hint");
        if (estimatorHint) {
            estimatorHint.classList.remove("hidden");
        }

        const soundChangeSetId = estimateButton.getAttribute("data-sound-change-set");
        if (!soundChangeSetId) return;
        estimateButton.addEventListener("click", async (event) => {
            event.preventDefault();
            const word = wordInput.value;
            try {
                setRunning();
                const [estimatedIpa] = await Promise.all([
                    (async () => {
                        const r = await estimate(word, soundChangeSetId);
                        return r;
                    })(),
                    new Promise(resolve => setTimeout(resolve, 500)) // ensure the saving state is visible for at least 500ms
                ]);
                ipaInput.value = estimatedIpa;
                setIdle();
            } catch (error) {
                console.error("Error estimating IPA:", error);
                setErrored();
            }
        });
    }
});