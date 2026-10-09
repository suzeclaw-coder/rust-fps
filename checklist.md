# Tidewater 1-to-1 Island Implementation Checklist

## 1. Terrain, Ocean & Atmosphere
- [x] Implement sloping beach profile & lush green island mountain backdrop in `src/maps/mod.rs` & `src/maps/tidewater.rs`
- [x] Configure solar lighting & sun angle (azimuth ~250° WSW, elevation ~27°, golden hour light, long eastward cast shadows)
- [x] Tune turquoise Caribbean water shader, seabed, and surf line breaking along the sandy beach

## 2. Pier, Boardwalk & Coastal Infrastructure
- [x] Build authentic pier: 2.6m wide deck at Y=2.3m, pile bents, perimeter guardrails, eastern berthing face with tyre fenders, ladder, bollards, cleats
- [x] Build pier entrance arch with hanging painted fish signboard and cage lanterns
- [x] Build Downeast lobster fishing boat moored at pier T-head East face
- [x] Build curved elevated timber boardwalk from pier foot up to village plaza with side branches, stringers, posts, and path lights

## 3. Village Architecture (All 23 Structures)
- [x] Implement procedural cottage generator supporting 1-2 stories, front porches (balusters / X-rails), gable/hip roofs (corrugated metal & thatch), Bahama/board shutters, trims, and stairs
- [x] Instantiate all 14 Cottages (A through N) with exact coordinates, dimensions, yaw, pastels, and roofs
- [x] Instantiate 3 beachfront Stilt Huts (S1, S2, S3) on elevated pilings (Y=3.0-3.15m) with fishnets & raw rims
- [x] Instantiate 4 Tool Sheds (shed1-shed4) with lean-to roofs and colored Z-braced doors
- [x] Instantiate Boathouse with slipway rails, flagpole, and interior boat cradle
- [x] Instantiate open-air Market Stall pavilion with thatched hip roof, turquoise counter, 3 fish display basins (ice/banana leaves), and hung trophy fish

## 4. Props, Foliage & Environmental Dressing
- [x] Add natural coconut palms with curved trunks, root flare, tiered fronds, and coconut bunches across beach & village
- [x] Add beach fire pit (basalt stone ring, charred wood, 2 log benches) on beach west of boardwalk
- [x] Add shoreline shipwreck (half-buried curved keel, 11 rib timbers) east of boathouse
- [x] Add upturned rowboats with leaning oars on beach sand and yard boat on trestles
- [x] Add fish cleaning table with cutting board, split fish, blood smear, fillet knife, tuna, and bucket
- [x] Add A-frame fish drying racks and T-post net racks with colorful mesh
- [x] Add laundry lines with blowing colorful clothes, elevated water cistern tanks on timber towers, woodpiles, lobster traps, and benches

## 5. Gameplay, Navmesh & Verification
- [x] Verify player spawn points, 22 zombie spawn points across 4 progressive zones
- [x] Verify 5 Mystery Box spots, 5 Perk machines, 2 Upgrade stations, 9 Wall buys, and extraction zone
- [x] Ensure all ground colliders are registered, no blocking glitches, and navmesh connectivity tests pass (`cargo test nav`)
- [x] Run lookdev and verify 1-to-1 visual match with reference screenshot

