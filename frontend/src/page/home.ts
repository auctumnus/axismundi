const greetings = [
  "welcome",
  "bienvenue",
  "willkommen",
  "欢迎",
  "ようこそ",
  "χαῖρε(τε)", // https://morethanone.info
  "selamat datang",
  "bem vinde",
  "witaj",
  "مرحبا",
  "salve",
  "स्वागतम",
  "benvengut",
  "nau mai",
];

const welcome = document.getElementById("welcome");

if (welcome)
  welcome.innerText = greetings[Math.floor(Math.random() * greetings.length)]!;

const updatePinButton = (form: HTMLFormElement, isPinned: boolean) => {
  const button = form.querySelector("button");
  const languageName = form.dataset.languageName || "language";
  const action = isPinned ? "Unpin" : "Pin";
  const tooltip = `${action} ${isPinned ? "from" : "to"} home`;
  const url = new URL(form.action);

  url.pathname = url.pathname.replace(
    /\/(?:pin|unpin)$/,
    `/${isPinned ? "unpin" : "pin"}`,
  );
  form.action = `${url.pathname}${url.search}`;
  form.dataset.pinned = String(isPinned);
  form.classList.toggle("pinned", isPinned);

  if (button) {
    button.setAttribute("aria-label", `${action} ${languageName} ${isPinned ? "from" : "to"} home`);
    button.setAttribute("data-tooltip", tooltip);
    button.querySelector(".tooltip")?.replaceChildren(tooltip);
  }
};

for (const form of document.querySelectorAll<HTMLFormElement>(".pin-language-form")) {
  form.addEventListener("submit", async (event) => {
    event.preventDefault();

    const button = form.querySelector<HTMLButtonElement>("button");
    if (button?.disabled) return;

    const isPinned = form.dataset.pinned === "true";
    button && (button.disabled = true);

    try {
      const response = await fetch(form.action, {
        method: form.method,
        credentials: "same-origin",
      });

      if (!response.ok) {
        throw new Error(`Failed to update pin: ${response.status}`);
      }

      updatePinButton(form, !isPinned);
    } catch (error) {
      console.error("Failed to update language pin:", error);
    } finally {
      button && (button.disabled = false);
    }
  });
}
