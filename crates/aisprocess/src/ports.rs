//! Built-in gazetteer of major Baltic + North-Sea ports.
//!
//! `(name, latitude, longitude)` — public-knowledge locations inside the sim
//! bbox (lat 50.5–66, lon −5–31). River / up-estuary ports use their seaward
//! approach coordinate; the router snaps each onto deep navigable water at
//! generation time, so the exact point only needs to be near open water.

pub const PORTS: &[(&str, f64, f64)] = &[
    // ── English / Scottish North-Sea coast ──
    ("London", 51.51, 1.05), // outer Thames estuary approach
    ("Felixstowe", 51.95, 1.32),
    ("Grimsby", 53.57, -0.07),
    ("Hull", 53.74, -0.29),
    ("Immingham", 53.63, -0.19),
    ("Newcastle", 55.01, -1.44),
    ("Aberdeen", 57.14, -2.07),
    // ── Continental North-Sea coast ──
    ("Rotterdam", 51.95, 4.05),
    ("Amsterdam", 52.46, 4.58), // IJmuiden sea lock approach
    ("Zeebrugge", 51.35, 3.19),
    ("Antwerp", 51.35, 3.30), // Scheldt seaward approach
    ("Bremerhaven", 53.55, 8.55),
    ("Wilhelmshaven", 53.60, 8.10),
    ("Hamburg", 53.89, 8.70), // Elbe mouth (Cuxhaven approach)
    ("Esbjerg", 55.47, 8.35),
    // ── Norwegian coast / Skagerrak ──
    ("Bergen", 60.39, 5.20),
    ("Stavanger", 58.97, 5.60),
    ("Kristiansand", 58.08, 8.00),
    ("Oslo", 59.60, 10.60), // outer Oslofjord
    // ── Kattegat / Danish straits ──
    ("Gothenburg", 57.68, 11.75),
    ("Aarhus", 56.15, 10.30),
    ("Frederikshavn", 57.44, 10.55),
    // ── Western Baltic ──
    ("Copenhagen", 55.70, 12.65),
    ("Malmo", 55.58, 12.95),
    ("Kiel", 54.40, 10.20),
    ("Rostock", 54.18, 12.10),
    ("Swinoujscie", 53.92, 14.28),
    // ── Southern / eastern Baltic ──
    ("Gdansk", 54.45, 18.70),
    ("Gdynia", 54.55, 18.60),
    ("Kaliningrad", 54.85, 20.10), // Baltiysk approach
    ("Klaipeda", 55.72, 21.10),
    ("Liepaja", 56.51, 20.95),
    ("Ventspils", 57.40, 21.50),
    ("Riga", 57.35, 23.80), // Gulf of Riga approach
    // ── Northern Baltic / Gulf of Finland / Bothnia ──
    ("Tallinn", 59.47, 24.75),
    ("Helsinki", 60.10, 24.96),
    ("Kotka", 60.42, 26.95),
    ("StPetersburg", 59.98, 29.10), // Gulf of Finland approach
    ("Stockholm", 59.30, 18.30),    // archipelago seaward approach
    ("Norrkoping", 58.55, 16.40),
    ("Turku", 60.30, 22.10), // archipelago approach
    ("Lulea", 65.55, 22.30),
];
