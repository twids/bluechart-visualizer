# BlueChart-visare – lokal prototyp

Visar verkligt kartinnehåll från `norden/gmappsupp-HEU800X-Norden-unlocked.img`
i webbläsaren. Testområdet omfattar Stavsnäs och Runmarö, med panorering,
zoom, djupområden, djupsiffror, ortnamn och förenklade sjömärken.
Ingen installation av Python-paket behövs. Python 3.10 eller senare krävs.

## Starta på Windows

Öppna PowerShell i projektmappen och kör:

```powershell
.\Start-Viewer.ps1
```

Öppna sedan <http://127.0.0.1:8765>. Avsluta servern med Ctrl+C.
Startskriptet använder Codex befintliga Python-runtime om den finns,
annars `python` från PATH. Det ändrar inga systeminställningar.
Om datorns skriptpolicy blockerar `.ps1`, starta direkt med Python:

```powershell
& "$env:USERPROFILE\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe" server.py
```

På andra datorer med Python installerat:

```text
python server.py --chart "/path/to/chart.img" --port 8765
```

Dra kartan med musen, scrolla eller använd +/− för zoom. Husknappen återställer
utsnittet. Tangentbord: piltangenter panorerar, +/− zoomar och Home återställer
när kartan har fokus. Håll pekaren över ett punktobjekt för djup, namn, typ och
koordinater. Lager kan slås på och av i sidofältet.

## Så fungerar läsningen

- Originalfilen öppnas med `rb` och ändras aldrig. Den kopieras inte.
- Ett litet index med filallokering, kartgränser och detaljnivåer läses vid start.
- Detaljindex läses först när kartdelen överlappar utsnittet; högst fyra sådana
  index behålls i minnet. Objekt hämtas med sökbara läsningar över även
  fragmenterade block och FAT-fortsättningsposter.
- Geometricachen har en uppskattad budget på 64 MiB och etikettcachen högst
  8 192 poster. Hela processens minne är större; se mätningen nedan.
- Alla beroenden ligger i Pythons standardbibliotek och webbläsaren.
  Inga externa kartplattor, teckensnitt eller tjänster används under visning.

Vid lokal start binder servern endast `127.0.0.1`, verifierar Host och serverar bara de tre
webbfilerna och två läs-API:er. IMG-filen kan inte hämtas över HTTP.
API:t begränsar visningen till 18.60–18.90° E, 59.18–59.35° N.
För stora detaljuttag ger en synlig anmärkning och högst 180 delområden.

## Läs-API

- `GET /api/metadata`: kartnamn, filstorlek, kartdelars gränser och
  detaljnivåer, startutsnitt, testgränser och begränsningar.
- `GET /api/features?bbox=west,south,east,north&bits=23`: avkodade objekt
  (`kind`, numerisk Garmin-`type`, `coords` i longitud/latitud, `label`,
  `bbox`, `tile`, eventuell `depth_m`), anmärkningar och lässtatistik.
  `fishing=0` hoppar över fiskekartans avkodning; `compact=1` utelämnar interna
  fält och avrundar endast överförda koordinater till sex decimaler.
  Webbläsaren skickar ett slumpat `client`-id och stigande `generation` så att
  servern kan avbryta äldre utsnitt som ersatts av nyare. HTTP-svar stöder gzip.

API:t använder grader WGS 84. Canvas använder Mercatorprojektion och
växlar mellan kartans upplösningsnivåer. Dessa API:er är prototypgränssnitt.

## Kartanalys 2026-10-26

Kartnamnet är **HXEU800X – Northern Europe v2014.0**. Behållaren har 32 KiB
block, 468 interna filer, 155 TRE/RGN-kartdelar och sammanlagt 651 311
delområdesposter. Detta identifierar kartan, men bekräftar inte generellt
stöd för alla produkter som marknadsförts som g2 eller g3.

Baslinje före prestandauppdateringen, för testuttaget
`18.68,59.23,18.81,59.30`, upplösning 24:

| Mätning på denna Windows-dator | Resultat |
|---|---:|
| Originalfil | 2 033 713 152 byte |
| Lästa data vid start | 428 658 byte |
| Lästa data inklusive första detaljuttaget | 3 032 904 byte |
| Största enskilda fysiska läsning | 32 768 byte |
| Första detaljuttag | cirka 1,06 s |
| Pythonprocessens arbetsminne efter uttaget | cirka 107 MB |
| Returnerade objekt | 5 261 |
| Djupsiffror | 171, från 0,7 till 86 m |
| Avkodningsanmärkningar för testuttaget | 0 |

Detta är en lokal mätning med operativsystemets filcache; ingen garanti för
kall diskläsning eller andra datorers tider. Geometricachen rymmer inte hela
detta detaljuttag samtidigt; upprepad hämtning kan därför läsa om data.
Maskinläsbara resultat sparas i `artifacts/measurements.json` av `measure.py`.
Se prestandauppdateringen nedan för den nu deployade versionen.

En Stavsnäs-punkt avkodas till cirka 59.28669° N, 18.70401° E. Den ligger
ungefär 125 m från [Värmdö kommuns publicerade hamnposition](https://www.varmdo.se/varmdohamnar/stavsnasvinterhamn.4.1524f1c618a8d1d9fee45c6d.html),
59°17.134′ N, 18°42.249′ E. Punkten är en ort-/kartetikett, inte samma
objekt som hamnpositionen. Det är en rimlighetskontroll av koordinater och
område, inte en verifiering av exakt kustlinje, djup eller navigationsnoggrannhet.

Webbläsartest i Edge/Chromium kontrollerar verklig ritning, panorering,
knapp-/hjulzoom, tangentbord, återställning, lager, smalare fönster och
frånvaro av JavaScript-fel och externa resursanrop. Bilder finns under
`artifacts/` efter testet.

## Tester och analys

```powershell
python -m unittest discover -s tests -v
$env:BLUECHART_INTEGRATION='1'
python -m unittest discover -s tests -v
python measure.py
python benchmark.py
python bluechart.py --bbox 18.68 59.23 18.81 59.30 --bits 24
```

De syntetiska testerna kräver ingen kartfil och täcker fragmenterade block,
fortsättningsposter, signerade koordinater och deltan, djupattribut,
ofullständiga poster samt HTTP-validering och filåtkomstbegränsning.
Integrationstestet använder användarens befintliga fil.

Valfritt webbläsartest, med servern igång och Playwright tillgängligt:

```text
node tests/browser-smoke.cjs
```

Standardkanal är installerad Microsoft Edge. `BLUECHART_BROWSER=chrome`
väljer Chrome. `BLUECHART_URL` väljer annan lokal testport.

## Begränsningar och nästa steg

Detta är en teknisk prototyp, inte navigationsunderlag. Symbolerna är
förenklade och djupområdenas färger saknar verifierad djupgränslegend.
Vissa objekt och texter utelämnas eller glesas ut för läsbarhet.
Fyrsektorer, Vision, rutter, NET-etiketter och Huffmankodad geometri
stöds inte. Extra attribut läses till rätt postslut men endast djupsiffrors
klassattribut tolkas fullt för presentation. Fel ger synliga anmärkningar.

Nästa etapp är noggrannare typ- och symboltolkning, kontroll mot en
oberoende kustlinje och större testtäckning över detaljnivåer och gränser.
Därefter kan testområdet utökas och GPX/rutter läggas till.

## Deployment på Unraid 2026-10-26

Visaren är publicerad på **https://garmin.widsell.nu**, öppet utan inloggning
enligt användarens uttryckliga val. TLS hanteras av befintliga Traefik/Pangolin.

- Dockhand-stack `bluechart`: `/mnt/user/appdata/dockhand/stacks/bluechart/compose.yaml`
  på Unraid, visad som `/mnt/stacks/bluechart/compose.yaml` i Dockhand.
- Källkodssnapshot: samma stackkatalogs `source/`. Projektets Compose-mall
  är `compose.dockhand.yaml`; `Dockerfile` innehåller ingen kartfil.
- Container `bluechart`, image `local/bluechart-viewer:20261026.3`, Python 3.12.15,
  användare `10001:10001`, skrivskyddat rotfilsystem, 512 MiB minnesgräns,
  en CPU, `unless-stopped`, healthcheck och roterade loggar.
- Originalfilen monteras direkt från projektets `norden/` på Unraid till
  `/charts/norden.img`, skrivskyddat. Filen är inte duplicerad eller inbakad i imagen.
- Endast Docker-nätverket `traefik`, inga publicerade hostportar.
  Pangolin-resurs `BlueChart` har backend `http://bluechart:8765`.
- Containern binder `0.0.0.0` internt. Serverns extra Host-lista innehåller
  exakt `garmin.widsell.nu` och `bluechart:8765`; lokal standardstart är fortsatt
  begränsad till loopback.

Verifierat: container running/healthy utan omstarter, HTTPS och metadata-API
HTTP 200, detaljerat kartuttag 5 261 objekt utan avkodningsanmärkningar samt
visuell kontroll och zoom/panorering genom den publika adressen.
Alla 13 läs-/HTTP-/integrationstester passerar efter prestandauppdateringen.

Vid uppdatering: uppdatera källkodssnapshoten, bygg en ny lokal image med ny
tagg, ändra stackens image och deploya endast `bluechart` genom Dockhand.
Eftersom imagen byggs lokalt ska **Pull images** vara avstängt i Dockhand.
Återställning: välj föregående image-tagg och redeploya. För att avpublicera,
inaktivera endast Pangolin-resursen `BlueChart` och stoppa stacken i Dockhand.
Kartfil och källkod ska bevaras.

### Prestandauppdatering

Profilering på Unraid visade CPU-arbete i bitavkodningen, en cache som evakuerade
samma kartdelar mellan förfrågningar samt dyr JSON-serialisering för att uppskatta
cachestorlek. Uppdateringen innehåller buffrad bitläsning, billigare storleksestimat,
en större men fortsatt begränsad cache, hopp över avstängda fiskelager,
avbrytning av ersatta förfrågningar och mindre gzip-komprimerade svar.
Panorering/zoom flyttar en cachad kartbild direkt medan nya objekt hämtas;
Mercatorkoordinater förberäknas när objekten anländer.

Kontrollerat med samma utsnitt (18.60–18.90° E, 59.226107–59.303885° N),
upplösning 22 och en CPU på Unraid:

| Mätning | Före | Efter |
|---|---:|---:|
| Första avkodning med fiskelager, samma 7 487 objekt | 2,74 s | 1,73 s |
| Upprepad avkodning med varm cache | — | 0,013 s |
| Svarsstorlek med samma lager | 5,68 MB okomprimerat | cirka 0,55 MB gzip |

Utan det normalt avstängda fiskelagret: 6 070 objekt, cirka 1,33 s första
avkodning, 0,010 s med varm cache och cirka 397 kB över nätverket. Två verkliga
HTTPS-hämtningar tog totalt 0,48 respektive 0,44 s, inklusive serverns
serialisering, komprimering och överföring; siffran i sidofältet visar endast
serverns kartuttag. Tider varierar med last och utsnitt.

Intern geometri före/efter jämfördes objekt för objekt via SHA-256:
`d40ac9640619b1f5a33df86774bd9385fb32a9415543ec853d1f5fce5a9a94b7`.
Ingen kartgeometri ändrades av parseroptimeringen. De sex decimalerna gäller
bara det kompakta HTTP-svaret (maximalt cirka 0,06 m avrundning per koordinat).

Testområdets gräns är kvar. Hela Norden kräver nästa etapp med geografiskt index,
beständig cache och hämtning per kartdel, så att stora utsnitt inte tvingar fram
fullständig ny avkodning. Detta är ännu inte implementerat.

## GitHub och kartdata

Källkod: https://github.com/twids/bluechart-visualizer

Kartfilen får inte laddas upp. `norden/`, alla IMG-filer, genererade artefakter,
kartdatabaser och arkiv är Git-ignorerade. Publicering sker med en uttrycklig
lista över kod-, test- och dokumentationsfiler; använd aldrig `git add -f`
för kartdata. Kartdata och skärmbilder ingår inte i repot.

Se `THIRD_PARTY.md` och `LICENSE` för parserreferenser och GPL-villkor.
