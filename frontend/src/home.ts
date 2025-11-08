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
    "مَرْحَبًا",
    "salve",
    "स्वागतम",
]

let welcome = document.getElementById("welcome");

if(welcome)
    welcome.innerText = greetings[Math.floor(Math.random() * greetings.length)]!;