# App design direction

A quiet desktop utility: the screen being shared is the primary content.
Keep connection actions visible, with quality and diagnostics behind a disclosure.
There is no marketing copy or simulated control in the application.

Palette: canvas #F7F8F7, surface #FFFFFF, text #1E2723, secondary #536058,
action #26734D, selection #D9ECDF. Use green for connection actions and selection;
reserve red for an actionable failure. No blue/dark application theme.

Desktop uses egui's bundled proportional sans; Android uses the platform sans
with medium weight for the selected tab. The local project page keeps its bundled
IBM Plex Sans/Sora fonts and adopts the same neutral/green palette.

Desktop layout:

```text
View / Share rail | Sharing status / full-screen action
                  | Address + Connect or Disconnect
                  | Connection status
                  | Live screen or quiet empty preview
```

Phone layout:

```text
Trinix
View | Share
Address or this phone's sharing address
Trust acknowledgement
One connection or sharing action
Status
Live screen / light empty preview
```

The two operations are separated so capture settings never compete with viewing.
Settings stay collapsed, errors remain visible, and the phone's empty preview is
light until real video arrives. Selected tabs are explicit. System capture consent
and a persistent Stop action remain part of Android sharing.

Review evidence: native desktop screenshot in images/desktop-light.png; Android
screenshots are produced by instrumentation. Screen sharing and viewing are real
operations. Input control and virtual display creation are not exposed as working
buttons until their implementations and acceptance checks exist.
