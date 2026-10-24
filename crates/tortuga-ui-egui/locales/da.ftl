# Tortugas brugerflade på dansk (R-I18N-2, D64). Oversat fra en.ftl.
#
# Ordliste, så de samme ord bruges overalt:
#   request: anmodning · response: svar · header: header · body: body
#   environment: miljø · variable: variabel · secret: hemmelig · server: server
#   project: projekt · workspace: arbejdsområde · folder: mappe · workflow: workflow
#   step: trin · script: script · test: test · capture: opsamling · example: eksempel
#   mock: mock · cookie: cookie · history: historik · settings: indstillinger
#   tab: fane · import: importér · send: send · save: gem · description: beskrivelse

## common
close = Luk
cancel = Annuller
send = Send
save = Gem
remove = Fjern
value = Værdi
name = Navn
and-separator = {" og "}
edit = Redigér
done = Færdig
choose = Vælg…
clear = Ryd
revert = Fortryd
open = Åbn
save-shortcut = Gem (Ctrl+S)
unsaved = Ikke gemt
move-up = Flyt op
move-down = Flyt ned
variable = Variabel
tests = Tests
console = Konsol
no-body = Ingen body.
passed = Bestået
failed = Fejlet
cancelled = Annulleret
running = Kører…
rename-ellipsis = Omdøb…
delete-ellipsis = Slet…
unsaved-changes = Ændringer, der ikke er gemt
git-branch = Git-gren: {$branch}
none = Ingen
project = Projekt
import = Importér
delete = Slet
sending = Sender…
headers = Headers
rename = Omdøb
create = Opret
history = Historik
settings = Indstillinger
error = Fejl
cut = Klip
copy = Kopiér
paste = Indsæt
folder = Mappe
reset = Nulstil
status = Status
add-variable = Tilføj variabel (tryk Enter)…
not-set = ikke sat
value-hint = værdi
none-yet = Ingen endnu.
header = Header
edit-project-defaults = Redigér projektets standarder…
edit-workspace-defaults = Redigér arbejdsområdets standarder…
parameter = Parameter
environment = Miljø
not-available-here = Ikke tilgængelig her.
add-server = Tilføj server (tryk Enter)…
servers = Servere
variables = Variabler
base-url = Basis-URL
default = Standard
on = Til
off = Fra

## dates

date-today = I dag
date-yesterday = I går
date-this-year = {$weekday} {$day}. {$month}
date-other-year = {$day}. {$month} {$year}
weekday-mon = man.
weekday-tue = tirs.
weekday-wed = ons.
weekday-thu = tors.
weekday-fri = fre.
weekday-sat = lør.
weekday-sun = søn.
month-jan = jan.
month-feb = feb.
month-mar = mar.
month-apr = apr.
month-may = maj
month-jun = jun.
month-jul = jul.
month-aug = aug.
month-sep = sep.
month-oct = okt.
month-nov = nov.
month-dec = dec.

## components/variable_row

variable-name-hint = navn
variable-remove = Fjern denne variabel

## shell/drafts

drafts-not-kept = Ændringer, der ikke er gemt, kunne ikke bevares til næste gang: {$error}
drafts-not-read = Ændringerne fra sidst, der ikke var gemt, kunne ikke læses: {$error}

## shell/toast

toast-dismiss = Klik for at lukke

## variables

variable-secret-hidden = hemmelig, skjult
var-from = Fra {$from}
var-ctrl-click-open = Ctrl+klik for at åbne, hvor den er defineret
var-not-defined = Ikke defineret
var-ctrl-click-define = Ctrl+klik for at definere den i projektets miljøer
var-source-runtime = en opsamling eller et script, gemt mens appen kører
var-source-workspace-runtime = en opsamling eller et script for arbejdsområdet, gemt mens appen kører
var-source-secret = denne computers hemmelige værdi
var-source-environment = miljøet “{$name}”
var-source-workspace-environment = arbejdsområdets miljø “{$name}”
var-source-folder = mappen “{$name}”
var-source-project = projektet
var-source-workspace = arbejdsområdet
var-source-dynamic = en dynamisk variabel: en ny værdi hver gang
dynamic-uuid = et tilfældigt UUID
dynamic-timestamp = tiden i sekunder siden 1970
dynamic-iso-timestamp = tiden i ISO 8601, UTC
dynamic-random-int = et tilfældigt heltal fra {$min} op til {$max}
workspace-vars-title = Arbejdsområdets variabler — {$name}
workspace-vars-captured = Opsamlet i denne session
workspace-vars-clear-captured = Ryd opsamlede
workspace-vars-clear-captured-hint = Glem de opsamlede værdier
workspace-vars-prefix = Udelad “{$prefix}”: “{$name}” bruges som {"{{"}{$rest}{"}}"}.

## mocks

mock-no-port = ingen port at prøve
mock-project-not-open = Projektet er ikke åbent.

## shell/appearance

code-font-fallback = Skrifttypen til kode kan ikke bruges, så den indbyggede bruges: {$error}
code-font-no-files = skrifttyper kan ikke læses fra filer her

## shell/response

saved-to = Gemt i {$path}
could-not-save = Kunne ikke gemme: {$error}
changes-previous-response = Det forrige svar → dette

## shell/sending

protected-send-title = Send {$method} til “{$env}”?
protected-send-message = “{$env}” er et beskyttet miljø: denne anmodning kan ændre dets data.

## shell/updates

update-available = Tortuga {$version} er klar
update-available-hint = Åbner udgivelsessiden med nyhederne og installationsfilerne

## components/shortcuts_dialog

shortcuts-title = Tastaturgenveje

## shell/mock

mock-started = Mockserveren for “{$name}” kører på port {$port}.
mock-start-failed = Mockserveren kunne ikke starte: {$error}
mock-stopped = Mockserveren for “{$name}” er stoppet.

## shell/dialogs

choose-ca-certificate = Vælg et CA-certifikat (PEM)
choose-code-font = Vælg en skrifttype til kode (.ttf, .otf)
dynamic-variable-info = {"{{"}{$name}{"}}"} får en ny værdi, hver gang den sendes.
scratch-no-variables = Kladdeanmodningen hører ikke til et projekt: gem den i et for at definere dens variabler.
code-font-unusable = Skrifttypen til kode kan ikke bruges: {$error}

## shell/layout

divider-show-request = Træk for at ændre anmodningens højde. Dobbeltklik for at vise anmodningen igen.
divider-response-full = Træk for at ændre anmodningens højde. Dobbeltklik for at give svaret hele pladsen.
divider-widths = Træk for at ændre bredderne. Dobbeltklik for lige brede.
divider-name = Skillelinje mellem anmodning og svar

## components/response_panel/filter

filter-help-json = JSONPath: $.name, [0], [-1], [1:3], [*], ..name, [?(@.price < 10)]
filter-help-xml = XPath: /a/b, //name, *, @id, text(), .., [1], [last()], [@id='7'], [price < 10]. Et navn uden præfiks matcher ethvert præfiks.
filter-not-json = Body er ikke gyldig JSON.
filter-not-xml = Body er ikke gyldig XML.
filter-only-json-xml = Kun JSON- og XML-bodies kan filtreres.

## components/response_panel/json_menu

json-menu-reading = Læser JSON…
json-menu-right-click = Højreklik på en linje med en værdi.
json-menu-add-test = Tilføj test
json-menu-capture-into = Opsaml i “{$name}”
json-menu-capture-hint = Tilføjer opsamlingen {$name} = {$path}
json-menu-copy-path = Kopiér sti

## components/response_panel/trend

trend-last-sends = Seneste {$count} afsendelser
trend-range = {": "}{$fastest} til {$slowest}
trend-failed = {", "}{$count} fejlede
trend-no-response = intet svar

## shell/disk

conflict-changed = Denne anmodning blev ændret på disken (for eksempel efter git pull), mens du havde ændringer, der ikke var gemt.
conflict-keep-mine = Behold mine
conflict-take-theirs = Tag deres
conflict-deleted = Denne anmodning blev slettet på disken, mens du havde ændringer, der ikke var gemt.
conflict-restore = Gendan med mine ændringer
conflict-discard = Kassér mine ændringer

## shell/fanout

fanout-save-first = Gem anmodningen i et projekt for at sende den til dets miljøer.
fanout-no-environments = Dette projekt har ingen miljøer at sende til.
fanout-confirm-title = Send {$method} til alle miljøer?
fanout-confirm-message = Det omfatter de beskyttede {$environments}: denne anmodning kan ændre deres data.

## components/key_value_table

kv-include = Medtag
kv-add = Tilføj {$name}

## shell/tabs

scratch-request = Kladdeanmodning
close-project-tabs-title = Luk fanerne for “{$name}”?
close-project-tabs-message =
    { $count ->
        [one] 1 af dets anmodninger har ændringer, der ikke er gemt.
       *[other] {$count} af dets anmodninger har ændringer, der ikke er gemt.
    } De går ikke tabt: åbner du anmodningen igen, vises de, indtil du gemmer eller fortryder.
close-project-tabs = Luk faner

## components/description

description-draft = Udkast fra anmodning
description-draft-hint = Start med det, anmodningen sender, og dens eksempler, og udfyld så resten
description-none = Ingen beskrivelse.
description-none-yet-draft = Ingen beskrivelse endnu. Klik på Redigér for at skrive en i Markdown, eller Udkast fra anmodning for at starte med det, den sender: den gemmes i filen og deles via git.
description-none-yet = Ingen beskrivelse endnu. Klik på Redigér for at skrive en i Markdown: den gemmes i filen og deles via git.
description-markdown-hint = Markdown: # overskrift, **fed**, *kursiv*, - liste, `kode`, ```kodeblok```, [link](https://…)

## components/environments/draft

env-needs-name = Hvert miljø skal have et navn.
env-duplicate = Der er to miljøer, der hedder “{$name}”.
env-variable-braces = Variabelnavne kan ikke indeholde krøllede parenteser: “{$name}”.
env-variable-duplicate = Der er to variabler, der hedder “{$name}”.
env-server-twice = “{$env}” nævner serveren “{$server}” to gange.
env-variable-duplicate-in = “{$env}” har to variabler, der hedder “{$name}”.

## components/palette

palette-command-hint = Skriv en kommando
palette-search-hint = Søg i anmodninger efter navn, mappe, projekt eller URL. Skriv > for kommandoer
palette-no-command = Ingen kommando passer.
palette-no-request = Ingen anmodning passer.
palette-more = {$count} mere: skriv for at indsnævre listen.

## components/sidebar/problems

project-missing = mangler
project-locate = Find…
project-locate-hint = Vælg mappen, hvor projektet ligger nu
project-copy-clone-url = Kopiér klon-URL
project-remove-from-workspace = Fjern fra arbejdsområdet
project-clone-hint = Klon det fra {$remote}, og brug så Find…
broken-file-delete = Slet denne fil
broken-file-error = Kunne ikke læse {$path}: {$error}. Ret filen i en editor, og brug så “Genindlæs fra disk”.

## workflows

workflow = Workflow
workflow-continue-on-failure = Fortsæt efter et fejlet trin
workflow-data-file = Datafil
workflow-data-none = ingen: én kørsel
workflow-data-hint = En CSV- eller JSON-fil i forhold til projektet: én kørsel pr. række med dens værdier som variabler
workflow-steps = Trin
workflow-steps-empty = Tilføj de anmodninger, der skal køres, i rækkefølge. Værdier, et trin opsamler, kan de næste bruge som {"{{"}variabler{"}}"}.
workflow-name = Workflowets navn
workflow-run = Kør
workflow-run-hint = Kør trinnene i rækkefølge med det aktive miljø (også ændringer, der ikke er gemt)
workflow-step-missing = ⚠ {$request} er ikke i projektet
workflow-captures =
    { $count ->
        [one] 1 opsamling
       *[other] {$count} opsamlinger
    }
workflow-step-has-tests = tests
workflow-step-remove = Fjern dette trin
workflow-step-open = Gå til anmodningen
workflow-step-checks = Kontroller…
workflow-step-checks-hint = Opsamlinger og tests for dette trin ud over anmodningens egne
workflow-step-name = Trinnets navn
captures = Opsamlinger
workflow-captures-hint = $.json.path, xpath://xml/path, header:Navn eller status; kan bruges som {"{{"}variabel{"}}"} i senere trin.
workflow-step-tests-hint = Køres efter anmodningens egne tests.
workflow-add-step = + Tilføj trin…
workflow-no-requests = Dette projekt har ingen anmodninger endnu.
workflow-add-step-hint = Tilføj en anmodning fra dette projekt som næste trin
workflow-not-run = Ikke kørt endnu
workflow-not-run-hint = Kør workflowet for at se resultatet af hvert trin her.
workflow-row = Række {$row}
workflow-skipped =
    { $count ->
        [one] 1 senere trin sprunget over efter fejlen.
       *[other] {$count} senere trin sprunget over efter fejlen.
    }
workflow-running-row-step = Række {$row}, trin {$step} af {$of} …
workflow-running-step = Trin {$step} af {$of} …
workflow-tests-passed =
    { $count ->
        [one] 1 test bestået
       *[other] {$count} tests bestået
    }
workflow-tests-failed = {$count} fejlet
workflow-steps-failed =
    { $count ->
        [one] 1 trin fejlet
       *[other] {$count} trin fejlet
    }
workflow-rows-done = {$done} af {$planned} rækker
workflow-iteration = Iteration {$n}
workflow-running-iteration-step = Iteration {$n}, trin {$step} af {$of} …
workflow-iterations-done = {$done} af {$planned} iterationer
workflow-iterations = Iterationer
workflow-iterations-hint = Hvor mange gange trinnene køres
workflow-iterations-by-rows = Datafilens rækker bestemmer, hvor mange gange trinnene køres
workflow-delay = Forsinkelse
workflow-delay-hint = Vent mellem trin, i millisekunder
workflow-keep-variables = Behold variabelværdier
workflow-keep-variables-hint = Start fra projektets variabler fra kørsler og behold det, scripts og opsamlinger sætter, på tværs af iterationer og efter kørslen. Fra: hver kørsel starter forfra og ændrer intet.
workflow-unsaved-run = Kørsel (ikke gemt)
workflow-save-as = Gem som workflow…
workflow-save-as-hint = Gem kørslen som en workflowfil i projektet, så den kan committes og køres igen
workflow-save-as-title = Gem som workflow
run-requests = Kør…
run-requests-hint = Kør anmodningerne i rækkefølge, som Postmans Run collection: vælg indstillinger, og tryk så Kør
palette-run-project = Kør projekt: {$name}
workflow-step-tests = tests {$passed}/{$total}
response-body = Svarets body
workflow-new-title = Nyt workflow
workflow-rename-title = Omdøb workflow
workflow-delete-title = Slet workflow?
workflow-delete-message = “{$name}” slettes fra disken. Dets anmodninger bliver.
workflow-deleted-drafts-dropped = Workflowet “{$name}” blev slettet, så dets ændringer, der ikke var gemt, blev kasseret.
workflow-choose-data = Vælg en CSV- eller JSON-datafil
workflow-protected-title = Kør “{$name}” mod “{$env}”?
workflow-protected-message = “{$env}” er et beskyttet miljø, og disse trin kan ændre dets data: {$steps}.
workflow-data-unreadable = Kunne ikke læse datafilen {$file}: {$error}

## components/sidebar

sidebar-projects = Projekter
sidebar-add = + Tilføj
sidebar-add-hint = Føj et projekt til dette arbejdsområde
sidebar-empty = Tilføj et projekt for at komme i gang. Projekter er almindelige filer, du kan committe til git ved siden af din kode.
scratch-request-hint = En anmodning, der ikke gemmes nogen steder, til hurtige enkeltstående kald
sidebar-mock-port = mock :{$port}
read-only = skrivebeskyttet
sidebar-workflows = Workflows
sidebar-workflows-hint = Anmodninger, der køres efter hinanden, med kontroller (R-FLOW-1)
sidebar-workflow-steps =
    { $count ->
        [one] 1 trin
       *[other] {$count} trin
    }
sidebar-repository-hint =
    Git-repository {$path}
    { $count ->
        [one] 1 projekt i dette arbejdsområde
       *[other] {$count} projekter i dette arbejdsområde
    }
sidebar-deleted-in-git = Slettet siden sidste commit. Commit sletningen, eller gendan filen med git.
sidebar-conflict-hint = Ændret på disken, mens du havde ændringer, der ikke var gemt: åbn den for at vælge
git-modified = Git: ændret siden sidste commit
git-new = Git: ny, ikke committet endnu
git-deleted = Git: slettet
git-conflicted = Git: flettekonflikt — ret den, og commit så

## import

import-postman-environments = Importér Postman-miljøer…
import-http-environments = Importér HTTP-miljøer…
import-bruno-environments = Importér Bruno-miljøer…
import-environments = Importér miljøer…
import-kind-openapi = OpenAPI-beskrivelse
import-kind-postman = Postman-samling
import-kind-http = HTTP-fil
import-kind-bruno = Bruno-samling
import-kind-insomnia = Insomnia-eksport
import-environments-postman-hint = Postman-miljøer eksporteret som JSON bliver til dette projekts miljøer
import-environments-http-hint = http-client.env.json, http-client.private.env.json eller en VS Code-indstillingsfil med REST Client-miljøer: de bliver til dette projekts miljøer
import-environments-bruno-hint = Bruno-miljøer (.bru-filer) fra en anden samling bliver også til dette projekts miljøer
import-from-bruno-folder = Fra mappen {$files} (Bruno-samling). Tortuga opretter et nyt projekt ud fra den; selve samlingen ændres ikke.
import-from-file = Fra {$files} ({$kind}). Tortuga opretter et nyt projekt ud fra den; selve filen ændres ikke.
import-from-files = Fra {$files}. Tortuga opretter et nyt projekt ud fra dem med en mappe pr. fil; selve filerne ændres ikke.
import-title-openapi = Importér OpenAPI
import-title-postman = Importér Postman-samling
import-title-http = Importér HTTP-fil
import-title-bruno = Importér Bruno-samling
import-title-insomnia = Importér Insomnia-eksport
import-add-environment-files = Tilføj miljøfiler…
import-project-folder = Projektmappe
import-folder-taken = Den mappe er allerede et Tortuga-projekt: vælg en anden.
import-inside-repository = I git-repositoryet i {$path}: commit projektet sammen med din kode.
import-not-as-written = Ikke importeret som skrevet ({$count})
import-requests =
    { $count ->
        [one] 1 anmodning
       *[other] {$count} anmodninger
    }
import-requests-in-folders =
    { $count ->
        [one] 1 anmodning
       *[other] {$count} anmodninger
    } i { $folders ->
        [one] 1 mappe
       *[other] {$folders} mapper
    }
import-row-requests = Anmodninger
import-row-environments = Miljøer
import-row-environment-files = Miljøfiler
import-row-secret-variables = Hemmelige variabler
import-secrets-fill-in = Hemmelige værdier bliver på denne computer: udfyld dem under Miljøer… efter importen.
import-secrets-found = Værdierne til disse blev fundet i filen: de gemmes på denne computer, ikke i projektets filer.
import-secrets-some-found = Værdierne til {$found} af disse blev fundet i filen: de gemmes på denne computer, ikke i projektets filer. Udfyld de andre {$missing} under Miljøer… efter importen.
import-environments-from = Fra {$files}. Basis-URL'er går til projektets servere; hemmelige værdier bliver på denne computer.
import-row-adds = Tilføjer
import-row-updates = Opdaterer
import-row-secret-values = Hemmelige værdier
import-environments-updated-keep = Miljøer, der opdateres, beholder det, filerne ikke sætter.
import-environments-secrets-found = Disse blev fundet i filerne: de gemmes på denne computer, ikke i projektets filer.
import-pick-http-environments = Vælg HTTP Client-miljøfiler eller VS Code-indstillinger med REST Client-miljøer
import-pick-postman-environments = Vælg Postman-miljøer (eksporteret som JSON)
import-pick-bruno-environments = Vælg Bruno-miljøer (.bru- eller .yml-filerne i en samlings environments-mappe)
import-failed-file = Kunne ikke importere {$file}: {$error}
import-turned-off = det er en {$kind}, og import af {$importer} er slået fra i Indstillinger
import-pick-openapi = Vælg en OpenAPI-beskrivelse (JSON eller YAML)
import-pick-postman = Vælg en Postman-samling (eksporteret som JSON)
import-pick-http = Vælg .http-filer (REST Client eller JetBrains HTTP Client) og deres miljøfiler
import-pick-insomnia = Vælg en Insomnia-eksport (v4 JSON eller v5 YAML)
import-pick-bruno = Vælg en Bruno-samling (mappen med bruno.json eller opencollection.yml)
import-pick-project-folder = Vælg eller opret en mappe til projektet
import-failed = Kunne ikke importere: {$error}
import-done = Importerede “{$name}”.
import-secrets-not-kept = Kunne ikke gemme de hemmelige værdier: {$error}
importer-openapi = OpenAPI-beskrivelser
importer-postman = Postman-samlinger
importer-http = .http-filer
importer-bruno = Bruno-samlinger
importer-insomnia = Insomnia-eksporter

## commands

cmd-add-projects = Tilføj eksisterende projekter…
cmd-new-project = Nyt projekt…
cmd-import-openapi = Importér OpenAPI…
cmd-import-postman = Importér Postman-samling…
cmd-import-insomnia = Importér Insomnia-eksport…
cmd-import-http = Importér .http-fil…
cmd-import-bruno = Importér Bruno-samling…
cmd-workspace-environments = Arbejdsområdets miljøer…
cmd-workspace-variables = Arbejdsområdets variabler…
cmd-workspace-defaults = Arbejdsområdets standarder…
cmd-scratch-detail = Gemmes ingen steder
cmd-environment = Miljø: {$name}
cmd-environment-mock = Miljø: Mock
cmd-current = aktuelt
cmd-mock-server = Mockserver…
cmd-stop-mock = Stop mockserver
cmd-start-mock = Start mockserver
cmd-run-workflow = Kør workflow: {$name}
cmd-new-workflow = Nyt workflow…
cmd-environments = Miljøer, servere og variabler…
cmd-project-defaults = Projektets standarder…
cmd-send = Send anmodning
cmd-send-everywhere = Send til alle miljøer
cmd-quick-open = Hurtig åbning: gå til en anmodning
cmd-revert = Fortryd ændringer
copy-as-curl = Kopiér som curl
cmd-new-request = Ny anmodning
cmd-workspace = Arbejdsområde: {$name}
cmd-runtime-variables = Kørselsvariabler…
cmd-settings = Indstillinger…
cmd-check-updates = Søg efter opdateringer
cmd-show-sidebar = Vis sidepanelet
cmd-hide-sidebar = Skjul sidepanelet
cmd-show-history = Vis historik

## components/fanout_dialog

fanout-title = Alle miljøer
fanout-baseline = sammenlignet med dette
fanout-same-shape = samme form

## shell/projects

new-request-name = Ny anmodning
new-folder-name = Ny mappe
delete-folder-message = “{$name}” og alt i den slettes fra disken.
delete-message = “{$name}” slettes fra disken.
delete-title = Slet?
new-project-pick-folder = Vælg eller opret en mappe til det nye projekt
add-projects-pick-folder = Tilføj projekterne i en mappe eller et repository
locate-project = Find “{$name}”
not-a-project = {$path} er ikke et Tortuga-projekt (den har ingen tortuga.toml).

## keys

key-up = Op
key-down = Ned
key-left = Venstre
key-right = Højre
or-separator = {" eller "}
command-toggle-comment = Udkommentér eller indkommentér linjer
command-block-comment = Blokkommentar
command-move-lines-up = Flyt linjer op
command-move-lines-down = Flyt linjer ned
command-copy-lines-up = Kopiér linjer op
command-copy-lines-down = Kopiér linjer ned
command-delete-lines = Slet linjer
command-indent-lines = Indryk linjer
command-outdent-lines = Ryk linjer ud

## components/changes_dialog

changes-title = Hvad er ændret
changes-nothing = Intet er ændret: samme status, headers og body.
changes-status = Status {$before} → {$after}
changes-time-size = Tid {$time_before} → {$time_after} · Størrelse {$size_before} → {$size_after}
changes-headers = Headers ({$count})
changes-body-same = Body: den samme
changes-body = Body ({$count})
changes-more = … og {$count} mere.
body = Body
changes-lines-differ =
    { $count ->
        [one] 1 linje er forskellig
       *[other] {$count} linjer er forskellige
    }, den første i linje {$first}.

## components/request_preview

preview = Forhåndsvisning
preview-hide = Skjul anmodningen, som den bliver sendt (Esc)
preview-show = Vis anmodningen, som den bliver sendt: variabler udfyldt, hemmeligheder skjult
preview-no-headers = Ingen headers.
preview-form-file = {$name}: fil {$file}{$content_type}
preview-body-cut = Body er længere; kun starten vises.

## shell/request_head

untitled-request = Unavngivet anmodning
save-to-project = Gem i projekt…
request-name = Anmodningens navn
request-unsaved-hint = Denne anmodning har ændringer, der ikke er gemt
curl-copy-failed = Kunne ikke kopiere som curl: {$error}
curl-read-failed = Kunne ikke læse curl-kommandoen: {$error}
curl-filled-in = Udfyldt fra curl-kommandoen. Ikke brugt: {$ignored}.
copied = Kopieret
copy-as-curl-hint = Kopiér denne anmodning som en curl-kommando med variablerne udfyldt

## response

image = Billede
image-too-large = For stort til at blive vist her. Brug “Gem i fil” for at beholde det.
image-scale = {$width} × {$height} px, vist i {$percent} %
cookies-none-set = Dette svar satte ingen cookies.
cookies-unreadable = Kunne ikke læse Set-Cookie: {$value}
cookies-stored-in-jar = Gemt i projektets cookie-krukke, medmindre deres domæne eller sti ikke passer til anmodningen.
cookies-open-jar = Åbn krukken…
cookies-domain-path = Domæne og sti
cookies-expires = Udløber
cookies-sent-to-domain = Sendes til {$domain} og dets underdomæner{$under}
cookies-this-host = Denne vært
cookies-sent-to-host = Sendes kun til den vært, der svarede{$under}
cookies-under-path = {", "}for stier under {$path}
cookies-under-request-folder = {", "}for stier under anmodningens mappe
cookies-session = Session
cookies-removes-it = Fjerner den
response-not-sent = Ikke sendt.
response-same-as-before = Samme som før
response-changes =
    { $count ->
        [one] 1 ændring
       *[other] {$count} ændringer
    }
response-changes-hint = Hvad der er ændret siden det forrige svar
response-cookies = Cookies
response-sending = Sender anmodning…
response-none-yet = Intet svar endnu
response-none-yet-hint = Send en anmodning for at se svaret her.
response-keys-hint = Ctrl+Enter sender, Ctrl+S gemmer
response-cancelled = Anmodningen blev annulleret.
response-failed = Kunne ikke få et svar
response = Svar
response-size = Størrelse
response-save-to-file = Gem i fil…
response-save-as-example = Gem som eksempel
response-save-as-example-hint = Behold dette svar som et eksempel, mockserveren svarer med
response-pretty = Formateret
response-binary = Binært indhold. Brug “Gem i fil” for at beholde det eller “Rå” for at se dets bytes.
response-filtering = Filtrerer…
response-truncated = Viser de første {$limit}. Brug “Gem i fil” for hele body.
response-pretty-hint = Vis body formateret
response-raw = Rå
response-raw-hint = Vis body, som den kom: uformateret tekst eller bytes som hex
response-no-tests = Ingen tests blev kørt. Tilføj nogle med test("navn", () => expect(res.status).toBe(200)) under fanen Scripts.
response-variables-set = Satte variabler
response-variable-removed = (fjernet)
response-variables-kept = Gemt for dette projekt, indtil Tortuga lukkes; de går forud for miljøets værdier.
response-filter = Filter
response-wrap = Ombryd
response-wrap-hint = Ombryd lange linjer (også under Indstillinger → Udseende)
response-next-match = Næste fund (Enter)
response-previous-match = Forrige fund (Shift+Enter)
response-no-matches = Ingen fund
response-match-of = {$current} af {$total}
response-search = Søg (Ctrl+F)
response-search-hint = Søg i body (Ctrl+F). Enter: næste fund, Shift+Enter: forrige.

## history

history-no-response-to-compare = Intet svar at sammenligne
history-compare-before = Sammenlign med afsendelsen før
history-compare-selected = Sammenlign med den valgte
history-select-for-compare = Vælg til sammenligning
history-filter = Filtrér historik
history-filter-hint = Søg efter navn, URL, status, projekt eller miljø
history-clear-one = Ryd {$name}
history-clear-all = Ryd al historik
history-clear-hint = Fjern historik gemt på denne computer
history-empty = Anmodninger, du sender, vises her. Historikken bliver på denne computer.
history-no-match = Intet i historikken passer.
history-older = Ældre poster: brug filteret for at finde dem.
history-read-failed = Kunne ikke læse historikken: {$error}
history-off = Historik er slået fra: nye afsendelser registreres ikke (Indstillinger → Historik).
history-first-send = Dette er anmodningens første afsendelse med et svar i historikken.
history-selected-for-compare = Valgt: højreklik nu på en anden afsendelse, og sammenlign den.
history-clear-title = Ryd historik?
history-clear-message = Historikken for {$name} fjernes fra denne computer.
history-clear-all-title = Ryd al historik?
history-clear-all-message = Al historik for anmodninger fjernes fra denne computer.
history-open-failed = Kunne ikke åbne historikposten: {$error}
history-compare-needs-response = Kun afsendelser, der fik et svar, kan sammenlignes.
history-body-cut = {" · "}body afkortet til 1 MB
history-past-label = Fra historikken: sendt {$sent}
history-back-to-latest = Tilbage til seneste
history-back-to-latest-hint = Vis det seneste svar på denne anmodning
history-restore = Gendan anmodning
history-restore-hint = Læg denne version af anmodningen i editoren som ændringer, der ikke er gemt
history-cleared = Historikken er ryddet.
history-clear-failed = Kunne ikke rydde historikken: {$error}

## shell

tls-off-warning = Kontrol af TLS-certifikater er slået fra. Svar kan komme fra en falsk server.
tls-off-change = Skift i indstillinger
dismiss = Luk
sidebar-show-hint = Vis sidepanelet (Ctrl+B)
sidebar-hide-hint = Skjul sidepanelet (Ctrl+B)
go-to-request = Gå til anmodning…
go-to-request-hint = Hurtig åbning (Ctrl+P). Ctrl+K viser alle kommandoer.
choose-upload-file = Vælg en fil, der skal uploades

## scripts

script-snippets = Kodestykker
script-rerun = Kør tests igen
script-rerun-hint = Kør opsamlinger og tests igen på det seneste svar uden at sende
script-rerun-disabled = Send anmodningen først: tests køres på dens svar
script-beautify = Forskøn
script-beautify-hint = Én sætning pr. linje med indrykning; kommentarer og strenge bevares
script-test-failed = fejlede
script-and-more = {" "}(og {$count} mere)
script-passed = {$count} bestået
script-hits =
    { $count ->
        [one] 1 stop
       *[other] {$count} stop
    }
script-gutter-hint = Klik på et linjenummer for at sætte eller fjerne et breakpoint
console-pre-request = Script før anmodning
console-step-tests = Trinnets tests
console-line = linje {$line}
console-breakpoint =
    ⏺ Breakpoint, linje {$line} — { $count ->
        [one] 1 variabel
       *[other] {$count} variabler
    }
console-no-variables = Ingen af scriptets variabler er synlige her.
console-array =
    {"[…]"} { $count ->
        [one] 1 element
       *[other] {$count} elementer
    }
console-object =
    {"{"}…{"}"} { $count ->
        [one] 1 nøgle
       *[other] {$count} nøgler
    }
snippet-set-variable = Sæt en variabel
snippet-set-header = Sæt en header
snippet-timestamp = Tidsstempel i en variabel
snippet-random = Tilfældigt tal i en variabel
snippet-log-request = Log anmodningen
snippet-status-200 = Status er 200
snippet-status-2xx = Status er 2xx
snippet-response-time = Svartid under 500 ms
snippet-body-json = Body er JSON
snippet-body-property = Body har en egenskab
snippet-save-field = Gem et felt i en variabel
snippet-log-body = Log body
snippet-save-cookie = Gem en cookie i en variabel
snippet-workflow-repeat = Workflow: gentag et trin, til det er færdigt
snippet-workflow-stop = Workflow: stop kørslen
snippet-read-xml = Læs en XML-body
scripts-pre-request = Før anmodning
scripts-pre-request-about = Køres, før variablerne udfyldes og anmodningen sendes.
scripts-tests-about = Køres, når svaret er kommet.
scripts-pre-request-reference = vars.get/set/unset/has/replaceIn/toObject, req.method/url/headers/body, req.setHeader(name, value), xml2Json(text), btoa/atob(text), info.requestName/iteration, workflow.next(step)/stop() i workflowkørsler, console.log/info/warn/error. Klik på et linjenummer for at sætte et breakpoint: Konsollen viser så variablerne der.
scripts-tests-reference = test(name, fn), test.skip(name, fn); expect(x).toBe/toEqual/toContain/toMatch/toHaveProperty/… (og .not) eller Chais expect(x).to.equal/.to.be.an("object")/.to.have.property/…; res.status, res.json(), res.header(name), res.cookie(name), res.time, res.body; xml2Json(text); btoa/atob(text); vars; info; workflow.next(step)/stop() i workflowkørsler; console.log. Klik på et linjenummer for at sætte et breakpoint: Konsollen viser så variablerne der.
scripts-reference = Reference
scripts-captures-hint = Lægger en værdi fra svaret i en variabel: $.json.path, xpath://xml/path, header:Navn eller status. Opsamlede værdier gemmes, indtil Tortuga lukkes, og går forud for miljøets værdier.

## tabs

expand = Fold ud
collapse = Fold sammen
tabs-show-project = Vis dette projekts faner
tabs-fold-project = Fold dette projekts faner sammen i mærket
tabs-close-project = Luk projektets faner
tabs-close-hint = Luk (ændringer, der ikke er gemt, bevares)
tabs-preview-hint = Forhåndsvisning: den næste anmodning, du åbner, erstatter den. Dobbeltklik for at holde den åben.
tabs-keep-open = Hold åben
tabs-close-others = Luk andre
tabs-close-right = Luk dem til højre
tabs-close-saved = Luk gemte faner
tabs-close-saved-hint = Luk alle faner uden ændringer, der ikke er gemt
tabs-all = Alle åbne faner

## state

files-unreadable-here = filer kan ikke læses her
ca-certificate-problem = CA-certifikat {$path}: {$error}
history-trim-failed = Kunne ikke afkorte historikken: {$error}
project-reload-failed = Kunne ikke genindlæse projektet: {$error}
secrets-read-failed = Kunne ikke læse hemmelige værdier: {$error}
workspace-file-read-failed = Kunne ikke læse arbejdsområdefilen: {$error}
workspace-file-save-failed = Kunne ikke gemme arbejdsområdefilen {$file}: {$error}
watch-failed = Ændringer på disken vises ikke automatisk (brug “Genindlæs fra disk”): {$error}
git-status-failed = Kunne ikke læse git-status: {$error}
cookies-save-failed = Kunne ikke gemme cookies: {$error}
cookies-read-failed = Kunne ikke læse gemte cookies: {$error}

## mock

mock-title = Mockserver — {$project}
mock-received = Modtaget
mock-running = Kører
mock-stopped-badge = Stoppet
mock-port = Port
mock-port-range = En port fra 1 til 65535
mock-restart-on-port = Genstart på denne port
mock-use-port = Brug denne port
mock-stop = Stop
mock-send-here = Send anmodninger hertil
mock-send-here-hint = Vælg miljøet “Mock”
mock-start = Start
mock-nothing-yet = Intet endnu.
mock-time = Tid
mock-request = Anmodning
mock-answered-by = Besvaret af

## settings

settings-close-pairs = Luk parenteser og anførselstegn
settings-close-pairs-hint = Skriver du ( [ {"{"} eller et anførselstegn, indsættes dets makker, eller markeringen pakkes ind i parret; en lukkende ved siden af markøren springes over.
settings-keys = Taster
settings-keys-hint = Klik på en tast, og tryk på en anden for at ændre den; Escape annullerer. I et scriptfelt går disse taster forud for appens egne. Ctrl er Cmd på macOS.
settings-keys-like = Taster som
settings-scheme-changed = {$scheme} (ændret)
settings-reset-all = Nulstil alle
settings-reset-all-hint = Tilbage til dette skemas taster for alle kommandoer
settings-scheme-replaces = Vælger du et skema, erstattes de taster, du har ændret.
settings-press-key = Tryk på en tast…
settings-key-unknown = {$key} (ukendt)
settings-key-change-hint = Klik, og tryk så på den nye tast
settings-key-remove = Fjern denne tast
settings-key-add = Tilføj en tast
settings-key-reset-hint = Tilbage til skemaets taster
settings-key-needs-modifier = {$key} skal bruges med Ctrl eller Alt, ellers ville tasten ikke længere skrive.
settings-key-taken = {$key} er allerede “{$command}”.
settings-key-elsewhere = Uden for scriptfelter gør {$key} stadig “{$does}”.
settings-appearance = Udseende
settings-editing = Redigering
settings-script-editor = Scripteditor
settings-features = Funktioner
settings-importers = Importører
settings-about = Om
settings-requests-tab-error = Fanen Anmodninger: {$error}.
settings-verify-tls = Kontrollér TLS-certifikater
settings-verify-tls-off = Slår du dette fra, kan alle mellem dig og serveren læse og ændre din trafik. Brug det kun til lokale testservere.
settings-default-timeout = Standard-timeout (ms)
settings-requests-hint = Arbejdsområder og projekter kan sætte deres egen timeout, omdirigeringer og cookies (deres “Standarder…”).
settings-insecure-hosts = Kontrollér ikke certifikater for disse værter:
settings-insecure-hosts-hint = Én pr. linje; *.example.test dækker dets underdomæner. Kun til testservere med selvsignerede certifikater. Gemt på denne computer: delte filer kan ikke slå kontrollen fra.
settings-insecure-hosts-warning = Trafik til disse værter kan læses og ændres af alle derimellem.
settings-proxy = Proxy
settings-proxy-system = Fra systemet
settings-proxy-manual = Manuel
settings-proxy-system-hint = Operativsystemets proxyindstillinger eller HTTP_PROXY, HTTPS_PROXY og NO_PROXY.
settings-proxy-none-hint = Anmodninger går direkte til serveren, uanset hvad systemets indstillinger siger.
settings-proxy-url = Proxy-URL
settings-no-proxy = Brug den ikke til disse værter:
settings-no-proxy-hint = Én pr. linje; example.com og *.example.com dækker begge dets underdomæner, og 10.0.0.0/8 et netværk. Til mitmproxy, Charles eller Fiddler eller en firmaproxy. Brugernavn eller adgangskode i URL'en understøttes ikke. Gemt på denne computer.
settings-ca-certificates = CA-certifikater
settings-ca-remove = Stol ikke længere på det
settings-add = Tilføj…
settings-ca-hint = PEM-filer, hvis certifikater der stoles på ud over systemets, f.eks. dit firmas CA til interne API'er. Gemmes kun på denne computer.
settings-theme = Tema
settings-like-system = Som systemet
settings-theme-light = Lyst
settings-theme-dark = Mørkt
settings-language = Sprog
settings-text-size = Tekststørrelse
settings-layout-below = Under anmodningen
settings-layout-beside = Ved siden af
settings-layout-automatic = Ved siden af, når der er plads
settings-density = Tæthed
settings-density-comfortable = Luftig
settings-density-compact = Kompakt
settings-code-font = Skrifttype til kode
settings-code-size = Kodestørrelse
settings-appearance-hint = Ctrl+= og Ctrl+- ændrer også tekststørrelsen; Ctrl+0 nulstiller den. Skrifttypen og størrelsen til kode gælder bodies, scripts og URL'er i forhold til tekststørrelsen.
settings-wrap-lines = Ombryd lange linjer i svar
settings-code-font-bundled = JetBrains Mono (indbygget)
settings-code-font-reset = Brug den indbyggede
settings-preview-tab = Åbn anmodninger i en forhåndsvisningsfane
settings-preview-tab-hint = Et enkelt klik viser en anmodning i forhåndsvisningsfanen (titlen i kursiv), som det næste enkelte klik erstatter; redigerer du den eller dobbeltklikker, bliver den. Fra: hver anmodning, du åbner, får en fane, der bliver.
settings-auto-save = Gem automatisk
settings-auto-save-hint = Ændringer i anmodninger og workflows gemmes et sekund efter, du holder op med at skrive, og når du åbner noget andet eller lukker appen. Ikke mens en fil er ændret på disken under dine ændringer (banneret spørger først), og aldrig kladdeanmodningen. Git er din fortryd.
settings-features-hint = Slå det fra, du ikke bruger, så appen holdes enkel. Intet slettes: en anmodning, der allerede bruger en funktion, viser den stadig, og en mockserver, der kører, kan stadig stoppes.
feature-scripts = Scripts, tests og opsamlinger
feature-examples = Eksempler og mockserveren
feature-workflows = Workflows
feature-docs = Beskrivelser
feature-scripts-hides = Fra skjuler fanen Scripts.
feature-examples-hides = Fra skjuler fanen Eksempler, “Gem som eksempel” og mockserveren.
feature-workflows-hides = Fra skjuler workflows i sidepanelet og paletten.
feature-docs-hides = Fra skjuler fanen Dok. og punkterne “Beskrivelse…”.
settings-history-on = Gem en historik over sendte anmodninger
settings-history-size = Poster pr. projekt
settings-history-lower = Sænker du den, fjernes de ældste poster, når du gemmer.
settings-history-off = Der registreres ikke mere. Historik, der allerede findes, bliver, indtil du rydder den i visningen Historik.
about-version = Version {$version}
about-built-for = Bygget til
about-project-files = Projektfiler
about-project-format = format {$format}; nyere åbnes skrivebeskyttet
about-workspace-files = Arbejdsområdefiler
about-format = format {$format}
about-licence = Licens
about-source = Kildekode
about-copy-version = Kopiér versionsoplysninger
updates-unsupported = Tortuga sender intet af sig selv: ingen telemetri, ingen søgning efter opdateringer. Den kontakter kun de servere, dine anmodninger går til.
updates = Opdateringer
updates-check-at-start = Søg efter en ny version, når appen starter
updates-check-now = Søg nu
updates-checking = Søger…
updates-up-to-date = Du har den nyeste version.
updates-hint = Spørger GitHub om den seneste udgivelse; intet om dig eller dine projekter sendes. Tortuga har ingen telemetri og kontakter ellers kun de servere, dine anmodninger går til.
settings-importers-hint = Slå de importører fra, du ikke bruger; de forsvinder fra “+ Tilføj” og kommandopaletten. Projekter, du har importeret tidligere, påvirkes ikke.

## dialogs

destination-top-level = {$project} (øverste niveau)
move-title = Flyt “{$name}”
move-to-folder = Til mappe
move = Flyt
save-request-title = Gem anmodning
save-request-no-project = Tilføj eller opret et projekt først (“+ Tilføj” i sidepanelet).
save-request-name-hint = f.eks. Vis brugere
copy-to-project-title = Kopiér “{$name}” til projekt
copy-to-project-none = Der er intet projekt, der kan ændres, at kopiere til.
drag-to-resize = Træk for at ændre størrelsen
new-project-title = Nyt projekt
new-project-hint = Tortuga tilføjer en tortuga.toml-fil til {$path}; anmodninger gemmes ved siden af den.
new-project-name = Projektnavn
new-project-git-init = Gør mappen til et git-repository
new-project-git-init-hint = Så projektet kan deles med git (git init)
add-projects-title = Tilføj eksisterende projekter
add-projects-looking = Leder efter projekter i {$path}…
add-projects-none = Ingen projekter (mapper med en tortuga.toml-fil) i {$path}.
add-projects-create-there = Opret et projekt der…
add-projects-found = Fundet i {$path}. Mapper, git ignorerer, blev sprunget over.
add-projects-already = {$path}  (allerede i arbejdsområdet)
add-projects-add =
    { $count ->
        [one] Tilføj 1 projekt
       *[other] Tilføj {$count} projekter
    }
curl-secrets-question = Denne anmodning bruger hemmelige værdier ({$names}). Kopiér den med dem vist som <secret:name>, eller medtag de rigtige værdier?
curl-secrets-hint = Kommandoer, du indsætter i chats, sager eller shell-historik, kan blive gemt længe.
curl-copy-masked = Kopiér skjult
curl-include-secrets = Medtag hemmelige værdier
runtime-vars-title = Kørselsvariabler — {$place}
clear-all = Ryd alle
cookies-title = Cookies — {$place}
cookies-jar-empty = Krukken er tom.
cookies-remove = Fjern denne cookie
cookies-host-only = Sendes kun til {$domain} for stier under {$path}
cookies-with-subdomains = Sendes til {$domain} og dets underdomæner for stier under {$path}

## environments

secret-help-project-secret = Hemmelig: værdierne bliver på denne maskine. Klik for at dele dem via projektets filer.
secret-help-project-shared = Delt via projektets filer. Klik for at gøre den hemmelig (værdierne bliver på denne maskine).
secret-help-workspace-secret = Hemmelig: værdien bliver på denne maskine. Klik for at dele den.
secret-help-workspace-shared = Delt med arbejdsområdet. Klik for at gøre den hemmelig (værdien bliver på denne maskine).
env-title = Miljøer — {$name}
env-none-yet = Ingen endnu. Tilføj et (for eksempel “Development”): alle projekter får det.
env-add = + Tilføj miljø
env-servers-and-variables = Servere og variabler
env-project-hint = Projektets servere og variabler, hemmeligt-markeringer og standarder
env-default-name = Miljø {$number}
env-remove = Fjern miljø
env-remove-hint = Fjerner det fra arbejdsområdet og fra alle projekter
env-colour = Farve
env-protected = Beskyttet
env-protected-hint = Spørger, før andet end GET, HEAD eller OPTIONS sendes hertil, i alle projekter
env-all-projects = Alle projekter
env-all-projects-hint = Værdier, alle projekter bruger, medmindre de sætter deres egne.
env-show-all = Vis alle projekter og de delte værdier
env-project-servers = {$name}: servere og variabler
env-read-only = {$name} (skrivebeskyttet)
env-project-values-hint = Dets egne værdier vinder; en tom bruger arbejdsområdets (gråtonet) og så standarden.
env-kept-in-file = arbejdsområdefilen {$file}
env-kept-on-machine = denne maskine (“Gem som fil…” i arbejdsområdemenuen deler den)
env-kept = Listen og de værdier, alle projekter deler, gemmes i {$kept}; hvert projekts værdier i dets egne filer, delt via git. Hemmelige (🔒) værdier bliver på denne maskine.
drag-to-reorder = Træk for at ændre rækkefølgen
env-remove-server-everywhere = Fjern denne server fra alle miljøer
env-workspace-secret = arbejdsområdets hemmelighed
env-default-hint = standard
env-remove-server = Fjern denne server fra projektet i alle miljøer
env-remove-variable = Fjern denne variabel fra projektet i alle miljøer
color-green = Grøn
color-teal = Blågrøn
color-blue = Blå
color-purple = Lilla
color-pink = Lyserød
color-red = Rød
color-orange = Orange
color-yellow = Gul
color-grey = Grå
env-server-name-hint = servernavn
env-remove-server-url = Fjern denne servers URL
env-shared-servers-hint = Hver URL går til alle projekters server med det navn.
env-shared-servers-hint-missing = Hver URL går til alle projekters server med det navn. Servere i projekterne: {$servers}.

## examples

example-no-name = (intet navn)
example-add = + Tilføj eksempel
example-default-name = Eksempel
example-earlier = Tidligere: spørges før dem nedenfor
example-later = Senere
example-remove = Fjern dette eksempel
example-status-range = En status fra 100 til 999
example-delay = Forsinkelse (ms)
example-delay-none = ingen
example-content-type-hint = Uden en Content-Type afgøres JSON, XML eller tekst ud fra body.
example-variables-hint = {"{{"}variabler{"}}"} udfyldes fra projektets aktive miljø.
example-answers-when = Svarer når
example-conditions-hint = Alle betingelser skal være opfyldt. Uden betingelser svarer dette eksempel på alle anmodninger, der når det.
example-query-parameter = Query-parameter
example-body-condition = Body ($.path eller xpath:…)

## levels

level-project = projektet
level-workspace = arbejdsområdet
level-folder = mappen “{$name}”
level-app = appens indstillinger

## auth

auth-describe-none = ingen godkendelse
auth-describe-basic = Basic-godkendelse
auth-describe-basic-as = Basic-godkendelse som {$username}
auth-describe-bearer = Bearer-token
auth-describe-bearer-token = Bearer-token {$token}
auth-describe-a-header = en header
auth-describe-header = headeren {$name}
auth-describe-header-value = headeren {$name}: {$value}
auth-inherit-project = Arv fra projektet
auth-inherit-from = Arv fra {$origin}
auth-uses-inherited = Denne anmodning bruger godkendelsen fra {$origin}: {$auth}.
auth-change-in-folder = Skift den med “Mappens standarder…” i den mappes menu.
auth-none-sent = Denne anmodning sender ingen godkendelse.
auth-saved-in-request = anmodningsfil
auth-kind-none = Ingen godkendelse
auth-kind-basic = Basic
auth-kind-bearer = Bearer-token
auth-no-header-added = Der tilføjes ingen Authorization-header.
auth-username = Brugernavn
auth-password = Adgangskode
auth-token = Token
auth-plain-credential = Denne værdi gemmes i {$saved_in} og deles via git. Brug i stedet en hemmelig variabel, f.eks. {"{{"}token{"}}"} — markér den som hemmelig (🔒) under Miljøer for at beholde den på denne maskine.
auth-header-replaces = En {$name}-header, du selv tilføjer, erstatter denne.
auth-header-needs-name = Der tilføjes ingen header, før den har et navn.

## request

request-server-no-url = Ingen URL til denne server i det aktive miljø
request-undefined-variables = Ikke defineret: {$names} — definér dem i projektets miljøer eller arbejdsområdets variabler.
request-unverified-host = ⚠ Certifikatet for {$host} kontrolleres ikke (Indstillinger → Anmodninger). Brug kun dette til testservere.
tab-params = Parametre
tab-auth = Godkendelse
tab-options = Valg
tab-scripts = Scripts
tab-examples = Eksempler
tab-docs = Dok.
request-content-type-hint = Content-Type sættes automatisk ud fra body-typen, medmindre du tilføjer den her.
request-send-hint = Send (Ctrl+Enter). Højreklik for at sende til alle miljøer.
request-path-variable = Stivariabel
request-path-variable-rename = Skift navnet i URL'en
request-path-variables-hint = Stivariabler kommer fra :navn-segmenter i URL'en. En tom værdi stopper afsendelsen.
request-inherited-headers-hint = Tilføj en header med samme navn ovenfor for at erstatte en, eller tilføj den slået fra for at udelade den.
request-method = HTTP-metode
request-params-offer =
    {$method} sender normalt data i sin body. Flyt { $count ->
        [one] query-parameteren
       *[other] de {$count} query-parametre
    } ind i den som
request-keep-in-url = Behold i URL
request-no-server = Ingen server
request-server-missing = {$id} (mangler)
request-url-hint = Indtast anmodningens URL
request-timeout = Timeout (ms)
request-from-level = {$value} (fra {$level})
request-timeout-hint = Lad stå tomt for at bruge projektets, arbejdsområdets eller appens indstilling.
request-jar-project = dette projekts cookie-krukke
request-jar-scratch = kladdens cookie-krukke (gemt, indtil appen lukkes)
request-use-cookies = Brug cookies fra {$jar}
request-cookies-off = Cookies er slået fra i {$level}, så denne anmodning sender og gemmer ingen.
request-cookies-hint =
    { $count ->
        [one] 1 cookie gemt
       *[other] {$count} cookies gemt
    }. Når det er slået fra, sendes ingen cookies fra krukken, og cookies i svaret gemmes ikke. En Cookie-header, du selv tilføjer, sendes altid.

## body

body-text = Tekst
body-form = Formular (URL-kodet)
body-beautify-hint = Indryk body; den skal være gyldig først
body-none = Denne anmodning har ingen body.
body-hint = Anmodningens body
body-field = Felt
body-full-path-warning = En fil uden for projektet beholder sin fulde sti: anmodningen virker kun på denne computer.
body-multipart-hint = Filer læses ved afsendelse. En fil i projektet gemmes i forhold til projektmappen, så kolleger også kan sende anmodningen. Content-Type med boundary sættes automatisk.
body-part = Del
body-value-or-file = Værdi eller fil
body-content-type = Indholdstype
body-add-part = Tilføj del
body-file-path-hint = sti/til/fil
body-pick-file = Vælg filen, der skal uploades
body-auto = auto
body-file = Fil
body-kind-hint = En tekstværdi eller en fil, der skal uploades

## host

history-not-kept-here = Historik gemmes ikke her.
mock-needs-desktop = Mockserveren kræver desktop-appen.
default-workspace = Mit arbejdsområde
secrets-in-file = Der er ikke noget lager til adgangsoplysninger i styresystemet ({$reason}). Hemmelige værdier gemmes i {$path}, som kun din brugerkonto kan læse.
secrets-file-unreadable = Den fil kunne ikke læses ({$error}), så hemmelige værdier gemmes kun, indtil appen lukkes.
secrets-no-folder = Der er ikke noget lager til adgangsoplysninger i styresystemet ({$reason}), og der blev ikke fundet nogen mappe til appdata. Hemmelige værdier gemmes kun, indtil appen lukkes.
secrets-page-only = Webversionen gemmer kun hemmelige værdier, indtil siden lukkes.
save-response-body = Gem svarets body

## workspaces

workspaces = Arbejdsområder
workspace-saved-in = Gemt i {$file}
workspace-new = Nyt arbejdsområde…
workspace-environments = Miljøer…
workspace-environments-hint = Miljøer, som alle projekter i dette arbejdsområde har, f.eks. Staging
workspace-variables = Variabler…
workspace-variables-hint = Variabler, som alle projekter i dette arbejdsområde kan bruge
workspace-defaults = Standarder…
workspace-defaults-hint = Headers, godkendelse, timeout, omdirigeringer og cookies, som alle projekter i dette arbejdsområde arver
workspace-file-synced = Holdes synkroniseret med {$file}. Vælg en anden fil for at flytte det.
workspace-save-as-file-hint = Skriv arbejdsområdet til en fil, du kan committe, så kolleger kan åbne det
workspace-save-as-file = Gem som fil…
workspace-open-file = Åbn arbejdsområdefil…
workspace-open-file-hint = Åbn et arbejdsområde, en kollega har gemt som en fil
workspace-remove = Fjern arbejdsområde…
workspace-remove-last = Der er altid mindst ét arbejdsområde
workspace-switch = Skift arbejdsområde
workspace-new-title = Nyt arbejdsområde
workspace-rename-title = Omdøb arbejdsområde
workspace-save-as-title = Gem arbejdsområde som
workspace-saved = Arbejdsområdet er gemt i {$file}.
workspace-open-file-title = Åbn en arbejdsområdefil
workspace-remove-title = Fjern arbejdsområde?
workspace-remove-message = “{$name}” fjernes fra denne maskine. Dets projekter og filer bliver, hvor de er.
workspace-add-environments = Tilføj miljøer…
workspace-mock-environment-hint = Sender anmodningerne fra projekter med en kørende mockserver til den
workspace-environment-picker-hint = Skifter alle projekter i arbejdsområdet til dette miljø
workspace-edit-environments-hint = Redigér miljøerne og hvert projekts værdier
mock-label-port = Mock (port {$port})
mock-label-servers = Mock ({$count} servere)

## shortcuts

shortcut-group-anywhere = Overalt
shortcut-group-tabs = Faner
shortcut-group-request = Anmodning
shortcut-group-scripts = Scripts
shortcut-send = Send anmodningen, eller kør det åbne workflow
shortcut-send-everywhere = Send anmodningen til alle miljøer
shortcut-save = Gem anmodningen eller workflowet
shortcut-quick-open = Gå til en anmodning
shortcut-commands = Alle kommandoer
shortcut-focus-url = Gå til URL'en
shortcut-environment = Skift miljø
shortcut-find = Søg i svaret
shortcut-sidebar = Skjul eller vis sidepanelet
shortcut-zoom = Større eller mindre tekst
shortcut-zoom-reset = Tekst i normal størrelse
shortcut-this-list = Denne liste
shortcut-previous-tab = Tilbage til den fane, der blev vist før
shortcut-next-tab = Næste eller forrige fane
shortcut-tab-at = Fanen på den plads
shortcut-last-tab = Den sidste fane
shortcut-close-tab = Luk fanen
shortcut-close-preview = Luk forhåndsvisningen under URL'en
shortcut-indent = Indryk linjerne, eller ryk dem ud

## sidebar

add-new-project-hint = Opret et projekt i en mappe efter eget valg
add-existing-hint = Find projekterne i en mappe eller et git-repository
add-openapi-hint = Opret et projekt ud fra en OpenAPI 3-beskrivelse (JSON eller YAML)
add-postman-hint = Opret et projekt ud fra en Postman-samling (v2.0- eller v2.1-JSON)
add-insomnia-hint = Opret et projekt ud fra en Insomnia-eksport (v4 JSON eller v5 YAML)
add-http-hint = Opret et projekt ud fra en .http-fil (VS Code REST Client, JetBrains HTTP Client)
add-bruno-hint = Opret et projekt ud fra en Bruno-samling (mappen med bruno.json eller opencollection.yml)
menu-new-folder = Ny mappe
menu-project-defaults = Standarder (headers, godkendelse, anmodninger)…
menu-description = Beskrivelse…
menu-mock-server = Mockserver (port {$port})…
menu-cookies = Cookies ({$count})…
menu-reload = Genindlæs fra disk
menu-file-manager = Åbn i filhåndtering
menu-terminal = Åbn i terminal
menu-folder-defaults = Mappens standarder…
menu-folder-defaults-hint = Headers, godkendelse og variabler, som alle anmodninger i denne mappe arver
menu-folder-description-hint = Hvad denne mappe indeholder, i Markdown
menu-duplicate = Duplikér
menu-move-to = Flyt til…
menu-copy-to-project = Kopiér til projekt…
menu-copy-to-project-hint = Kopiér til en mappe i et andet projekt; denne forbliver, som den er
menu-actions = Handlinger

## defaults

defaults-file-folder = mappefil (_folder.toml)
defaults-file-project = projektfil (tortuga.toml)
defaults-file-workspace = arbejdsområdefil
defaults-file-workspace-local = arbejdsområde (på denne maskine)
defaults-variables-hint = Standarder for {"{{"}variabler{"}}"} i denne mappes anmodninger. En værdi fra det aktive miljø eller en hemmelig værdi går forud for disse.
defaults-title-workspace = Arbejdsområdets standarder — {$name}
defaults-title-project = Projektets standarder — {$name}
defaults-title-folder = Mappens standarder — {$name}
defaults-description = Beskrivelse
defaults-explain-folder = Alle anmodninger i denne mappe og dens undermapper arver disse før projektets standarder. En anmodning (eller en undermappe) kan erstatte en header ved at bruge samme navn og kan vælge sin egen godkendelse.
defaults-explain-project = Alle anmodninger i dette projekt arver disse frem for arbejdsområdets. En anmodning kan erstatte en header ved at tilføje en med samme navn (eller slå den fra) og kan vælge sin egen godkendelse.
defaults-explain-workspace = Alle projekter i dette arbejdsområde arver disse efter deres egne: en projekt-, mappe- eller anmodningsheader med samme navn erstatter en af disse, og deres egen godkendelse vinder. Kladdeanmodningen bruger dem ikke.
defaults-follow-redirects = Følg omdirigeringer
defaults-cookies = Send og gem cookies
defaults-requests-hint-workspace = Tom eller “Arv” bruger appens indstillinger. Projekter kan sætte deres egne, og en anmodning kan stadig sætte sin egen timeout og slå cookies fra.
defaults-requests-hint-project = Tom eller “Arv” bruger arbejdsområdets værdi og ellers appens indstillinger. En anmodning kan stadig sætte sin egen timeout og slå cookies fra.
defaults-inherit-on = Arv: til (fra {$level})
defaults-inherit-off = Arv: fra (fra {$level})
defaults-inherit = Arv
defaults-auth-folder-inherits = Anmodninger i denne mappe bruger godkendelsen fra en overordnet mappe eller fra projektet.
defaults-auth-project-inherits = Anmodninger i dette projekt bruger arbejdsområdets godkendelse, hvis det har en.
defaults-auth-workspace-none = Anmodninger sender ingen godkendelse, medmindre deres projekt sætter en.
defaults-auth-folder-none = Anmodninger i denne mappe sender ingen godkendelse, uanset hvad en overordnet mappe eller projektet sætter. En anmodning kan stadig vælge sin egen.
defaults-auth-project-none = Anmodninger i dette projekt sender ingen godkendelse, heller ikke når arbejdsområdet sætter en. En mappe eller anmodning kan stadig vælge sin egen.
