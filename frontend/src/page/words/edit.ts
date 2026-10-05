const estimate = async (
  url: string,
  fields: FormData,
  signal?: AbortSignal,
) => {
  const body = new URLSearchParams();
  for (const [name, value] of fields) {
    if (typeof value === "string") body.append(name, value);
  }
  const response = await fetch(url, { method: "POST", body, signal });
  const document = new DOMParser().parseFromString(
    await response.text(),
    "text/html",
  );
  if (response.ok) {
    const ipa = document.querySelector<HTMLInputElement>('input[name="ipa"]');
    if (ipa) return ipa.value;
  }
  const errorText = document.querySelector("p.error")?.textContent?.trim();
  throw new Error(errorText || "Failed to estimate IPA");
};

const getOrCreateFieldErrors = (section: HTMLElement): HTMLUListElement => {
  let ul = section.querySelector<HTMLUListElement>("ul.field-errors");
  if (!ul) {
    ul = document.createElement("ul");
    ul.className = "field-errors";
    const ipaEstimator = section.querySelector(".ipa-estimator");
    if (ipaEstimator) {
      section.insertBefore(ul, ipaEstimator);
    } else {
      section.appendChild(ul);
    }
  }
  return ul;
};

const clearFieldErrors = (section: HTMLElement) => {
  const ul = section.querySelector("ul.field-errors");
  if (ul) ul.remove();
};

const debounce = (func: Function, delay: number) => {
  let timeoutId: number | null = null;
  return (...args: any[]) => {
    if (timeoutId) {
      clearTimeout(timeoutId);
    }
    timeoutId = window.setTimeout(() => {
      func(...args);
      timeoutId = null;
    }, delay);
  };
};

document.addEventListener("DOMContentLoaded", () => {
  const wordInput = document.getElementById("word") as HTMLInputElement | null;
  const ipaInput = document.getElementById("ipa") as HTMLInputElement | null;

  if (!wordInput || !ipaInput) return;

  const estimateButton = document.getElementById(
    "estimate-ipa",
  ) as HTMLButtonElement | null;
  if (estimateButton) {
    const ipaSection = ipaInput.closest("section") as HTMLElement | null;

    const setRunning = () => {
      estimateButton.disabled = true;
      estimateButton.ariaLabel = "Estimating IPA";
      estimateButton.classList.add("loading");

      if (ipaSection) clearFieldErrors(ipaSection);
    };

    const setIdle = () => {
      estimateButton.disabled = false;
      estimateButton.ariaLabel = "Estimate IPA";
      estimateButton.classList.remove("loading");
    };

    const setErrored = (message: string) => {
      estimateButton.disabled = false;
      estimateButton.ariaLabel = "Error estimating IPA";
      estimateButton.classList.remove("loading");
      if (ipaSection) {
        const ul = getOrCreateFieldErrors(ipaSection);
        ul.innerHTML = "";
        const li = document.createElement("li");
        li.textContent = message;
        ul.appendChild(li);
      }
    };

    const estimatorHint = document.getElementById("ipa-estimator-hint");
    if (estimatorHint) {
      estimatorHint.classList.remove("hidden");
    }

    const form = estimateButton.form;
    const estimateUrl = estimateButton.getAttribute("formaction");
    if (!form || !estimateUrl) return;

    let controller: AbortController | null = null;

    const e = async () => {
      const extraInputs = form.querySelectorAll<
        HTMLInputElement | HTMLTextAreaElement
      >('.extra-editor input, .extra-editor textarea, textarea[name="extra"]');
      if ([...extraInputs].some((input) => !input.validity.valid)) return;
      const fields = new FormData(form);
      if (controller) {
        controller.abort();
      }
      try {
        setRunning();
        const [estimatedIpa] = await Promise.all([
          (async () => {
            controller = new AbortController();
            const r = await estimate(estimateUrl, fields, controller.signal);
            return r;
          })(),
          new Promise((resolve) => setTimeout(resolve, 500)), // ensure the saving state is visible for at least 500ms
        ]);
        ipaInput.value = estimatedIpa;
        setIdle();
      } catch (error) {
        if (error instanceof DOMException && error.name === "AbortError") {
          // Ignore abort errors
          return;
        }
        console.error("Error estimating IPA:", error);
        setErrored(
          error instanceof Error ? error.message : "Failed to estimate IPA",
        );
      }
    };

    // The Extra editor flushes drafts in the form's capture listener before
    // this listener reads FormData. Ordinary Save submissions stay native.
    form.addEventListener("submit", (event) => {
      if (event.submitter !== estimateButton || event.defaultPrevented) return;
      event.preventDefault();
      e();
    });

    const wordEvent = debounce(e, 500);

    wordInput.addEventListener("input", wordEvent);

    ipaInput.addEventListener("input", () => {
      wordInput.removeEventListener("input", wordEvent);
      setIdle();
    });
  }
});
