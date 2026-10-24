**BEYOND THE STARS  |  GAME DESIGN DOCUMENT** 

# **BEYOND THE STARS** 

### **FULL GAME DESIGN PLAN** 

_A sci-fi factory automation, exploration, colony management, and procedural space-expansion game_ 

Design status: concept specification • Working title 

Working design plan • Concept and systems specification

**BEYOND THE STARS  |  GAME DESIGN DOCUMENT** 

## **Document overview** 

This document consolidates the current design decisions into a single working plan. Confirmed rules are treated as design constraints; proposed details are identified as recommendations that still need balancing or validation. 

|**Area**|**Summary**|
|---|---|
|Core loop|Explore → survey → prepare → deliver → build → automate →<br>research → expand.|
|Perspective|Overhead, god-view construction and management; the player is<br>not a vulnerable walking character.|
|Scale|Compact planets → star systems → galaxies → intergalactic<br>space.|
|Key distinction|Automation operates within locations; physical logistics connect<br>locations; research expands civilization-wide capabilities.|
|Current status|Concept and systems planning only; no implementation or code<br>is included.|



## **1. Core vision** 

The game begins on a handcrafted home planet in a handcrafted starting star system. The player builds an industrial base, discovers nearby worlds, transports equipment and people, and gradually develops a civilization spanning procedurally generated systems, galaxies, and eventually intergalactic space. 

The game should preserve the satisfying, hands-on factory-building experience as the scale grows. Later progression introduces new operational problems—not just higher costs—such as throughput planning, distributed production, energy transmission, long-range cargo networks, and specialized colonies. 

#### **Four design pillars** 

|**Pillar**|**Player activity**|**Long-term purpose**|
|---|---|---|
|Build|Construct mines, factories, power, belts,<br>habitats, research facilities, and space<br>infrastructure.|Create efficient and scalable production.|
|Explore|Scan systems, survey planets, discover<br>resources and hazards.|Find useful destinations and new<br>opportunities.|
|Expand|Physically transport ships, machines,<br>components, supplies, and people.|Turn discoveries into working outposts<br>and colonies.|
|Transcend|Unlock advanced energy, propulsion,<br>exotic materials, and intergalactic travel.|Extend industrial civilization into new<br>scales.|



## **2. Perspective, world scale, and navigation** 

#### **Overhead construction view** 

- The player observes the world from above and places buildings, routes conveyors, configures machines, and oversees expansion. 

- The player does not directly control a vulnerable character walking around the planet. 

- Planets are compact enough to cross and build across quickly, while retaining visual variety and meaningful terrain. 

- Building placement snaps to a grid by default, with rotation, terrain adaptation, and supported irregular footprints. 

- The interface should remain readable at normal zoom and across planetary, system, and galaxy scales. 

Working design plan • Concept and systems specification 

**BEYOND THE STARS  |  GAME DESIGN DOCUMENT** 

#### **Map navigation** 

- Pressing M opens map view. 

- Zooming out from a planet reveals its parent star system; zooming farther reveals the galaxy where appropriate. 

- Previously discovered destinations can be selected without a fixed route network, subject to available ships and travel resources. 

- Map access is not cargo teleportation: physical goods and people still require transport. 

- The resource bar appears only in planet view because inventories are local rather than universal. 

#### **Travel animation and generation messages** 

Travel uses animated journeys. New destinations are procedurally generated when needed; funny messages and fictional physicist quotations appear only during genuine generation, remain visible briefly afterward, and then disappear. Revisited destinations do not regenerate or replay generation messages. The simulation’s time controls should interact consistently with travel and generation; this behavior needs a final rule during balancing. 

## **3. Core rules and constraints** 

|**System**<br>Player|**Rule**<br>Invulnerable overhead overseer; no player-death or respawn<br>mechanic.|
|---|---|
|Time|Global pause, normal speed, and fast-speed levels: 2×, 4×, 8×,<br>16×, 32×, 64×, 128×, 256×, and further levels through<br>progression.|
|Instant-Warp|A time-control ability that advances simulation time until a<br>selected scout ship or other vessel reaches its destination.|
|Inventories|Resources belong to individual planets, ships, or explicitly<br>defined storage locations.|
|Shipping|Physical goods and people must travel on suitable ships; cargo<br>cannot appear at destinations automatically.|
|Navigation|Known destinations are selectable on the map; travel still<br>requires suitable ships and operating resources.|
|Discovery|Scanning reveals partial information; true planetary<br>characteristics require adequate surveying.|
|Colonization|Required surveys and delivery of all necessary resources must<br>be completed before construction begins.|
|Industry|Robots can run industrial operations without resident humans.|
|People|Required for research, ships, and other explicitly human-<br>dependent activities.|
|Housing|A house automatically provides enough basic food and<br>resources for its residents to survive, even if abandoned.|
|Colony activation|The player must visit a colony and establish its work systems in<br>person; human-dependent work remains inactive until<br>established.|
|Deposits|Resource reserves are infinite, but extraction rate is limited by<br>time, machinery, and technology.|
|Power|Starts locally; interplanetary and interstellar energy transmission<br>unlock through research and require infrastructure.|
|Power loss|Unpowered machines stop immediately; inventories and<br>production progress are preserved; machines resume<br>automatically when power returns.|
|Research|Staffed research facilities generate research points over time<br>into a civilization-wide shared pool.|
|Technology|Branching tree with prerequisites and specialization paths.<br>Projects can be switched freely and accumulated progress is<br>preserved.|
|Hostile life|Some planets contain hostile organisms. Players choose<br>between eliminatingthem and defending/coexistingwhere|



Working design plan • Concept and systems specification 

**BEYOND THE STARS  |  GAME DESIGN DOCUMENT** 

possible. 

## **4. Core gameplay loop** 

1. Discover: scan stars, planets, and unknown regions. 

2. Survey: reveal resources, terrain, and hazards sufficiently to plan a settlement. 

3. Prepare: manufacture ships, machines, supplies, and colonization equipment. 

4. Deliver: transport the required cargo and people physically. 

5. Build: construct extraction, power, production, and housing systems. 

6. Automate: connect machines with conveyors and inserters; improve throughput. 

7. Research: staff facilities, generate research points, and unlock new capabilities. 

8. Expand: use those capabilities to reach more distant and complex locations. 

This loop repeats at every scale. Early challenges center on basic production; later challenges concern bottlenecks, energy distribution, shipping capacity, specialization, and coordinating large networks. 

## **5. Resource and production economy** 

|**Tier**|**Example materials andproducts**|**Role**|
|---|---|---|
|1. Raw materials|Iron ore, copper ore, stone, water, carbon,<br>ice, atmosphericgases|Inputs for early extraction and processing.|
|2. Refined materials|Metal plates, wire, glass, alloys,<br>processed chemicals, fuel|Intermediate production and basic<br>construction.|
|3. Industrial components|Motors, circuits, pipes, pumps, robotic<br>components, machine parts|Machines, vehicles, and industrial<br>infrastructure.|
|4. Advanced technology|Computers, navigation systems,<br>advanced reactors, precision instruments|Research, automation, and advanced<br>systems.|
|5. Spacefaring technology|Rocket engines, launch systems,<br>interstellar drives, colony equipment|Expansion and cargo transport.|
|6. Exotic materials|Rare matter, unusual energy sources,<br>advanced physics materials|Late-game propulsion and intergalactic<br>travel.|



#### **Infinite reserves, finite throughput** 

Deposits never run out. Their extraction rate is constrained by machinery, power, and technology. Upgrades increase throughput without forcing players to abandon exhausted deposits. This preserves optimization challenges without repetitive depletion. 

#### **Production chains** 

A typical chain is: mine ore → smelt metal → manufacture components → assemble machines or spacecraft. Each chain must account for input throughput, output throughput, machine speed, power availability, and storage capacity. 

## **6. Factory construction and automation** 

#### **Placement and local logistics** 

- Buildings snap to a grid by default; rotation, terrain adaptation, and irregular footprints are supported where appropriate. 

- Conveyor belts are the primary local logistics system. Inserters or equivalent devices load and unload machines. 

- Conveyor belts use fixed speed tiers; faster tiers unlock through research. 

Working design plan • Concept and systems specification 

###### **BEYOND THE STARS  |  GAME DESIGN DOCUMENT** 

- Throughput bottlenecks should be visible and understandable, including overloaded belts, slow machines, full output buffers, and missing inputs. 

- Conveyors are for factory-scale movement. Cargo between planets and star systems still requires ships. 

#### **Machine generations and modules** 

- Research unlocks improved machine generations with better throughput, efficiency, or new capabilities. 

- Players manufacture and install replacement machines to upgrade a production line. 

- Modules customize individual machines, for example speed, efficiency, productivity, or specialized roles. 

- Upgrades must retain meaningful trade-offs and interact with belt throughput and power constraints. 

#### **Power management** 

- Each planet starts with its own generation and distribution network. 

- Power outages immediately stop unpowered machines; state and contents are preserved. 

- Machines resume automatically when sufficient power returns. 

- Research later unlocks interplanetary transmission and then interstellar energy networks. 

- Long-range transmission requires constructed infrastructure and should have meaningful capacity or operating constraints. 

## **7. Exploration, scanning, and scouting** 

|**Level**|**Example information**|
|---|---|
|Basic scan|Nearby systems, star type, planet count, and rough planet sizes.|
|Intermediate scan|Better orbital details, broad environmental categories, and likely<br>resource indicators.|
|Advanced scan|More systems per operation and more detailed information<br>within a system.|
|Specialized scan|Improved detection of rare deposits, unusual phenomena, and<br>potentially hazardous environments.|



Scanning is imperfect by design. It reveals useful information without replacing full surveys. Exact scan range and reveal quantities should be balanced later. 

#### **Scout ship roles** 

- Scouts are generally lighter, cheaper to build and operate, and sometimes faster than heavier ships. 

- Modules define capabilities such as surveying, scan improvement, hazard detection, limited supply carrying, or outpost deployment. 

- A scout visit makes a location accessible on the map, but does not automatically supply or colonize it. 

- Planetary properties remain uncertain until sufficient surveys are completed. 

#### **Colonization sequence** 

9. Scout reaches a system and reveals basic information. 

10. Survey the target until required planetary characteristics are known. 

11. Manufacture and load construction equipment, supplies, machines, and people as needed. 

12. Deliver all required resources by ship. 

13. Begin construction once survey and delivery conditions are satisfied. 

14. Visit the colony and establish its human-dependent work systems in person. 

Working design plan • Concept and systems specification 

**BEYOND THE STARS  |  GAME DESIGN DOCUMENT** 

## **8. Ships, cargo, and physical logistics** 

|**Ship role**|**Primary use**|**Trade-of**|
|---|---|---|
|Scout|Survey, reconnaissance, possible outpost<br>deployment|Low cost and cargo capacity; specialized<br>modules.|
|Cargo transport|Move materials, machines, supplies, and<br>people|Higher capacity, with greater construction<br>and operating needs.|
|Automated freighter|Scheduled routes and recurring bulk<br>shipments|Requires logistics technology and<br>ships/cargo to be available.|
|Deep-space carrier|Large-scale transport across distant<br>systems|High capacity and substantial resource<br>requirements.|
|Intergalactic vessel|Travel between galaxies|Late-game propulsion and exotic-material<br>requirements.|



These are progression roles rather than necessarily five mandatory hull types. Ship specialization and upgrades can be explored during balancing. 

#### **Cargo and travel rules** 

- Ships have finite capacity; people occupy capacity and require appropriate transport conditions. 

- Cargo must exist and be loaded before a shipment can depart. 

- Travel costs are resource costs associated with ship construction, fuel, and operation—not a separate click-to-travel fee. 

- Automated routes must respect ship availability, source inventories, cargo capacity, and operating resources. 

- Known destinations are selectable without a fixed route network, but navigation convenience never creates free cargo transport. 

## **9. Colonies, people, and automation** 

#### **Robotic industry** 

Robots can run mines, refineries, factories, construction machinery, material handling, and other designated industrial operations without a resident human workforce, provided power, inputs, and operating conditions remain adequate. 

#### **People and population** 

- People must be physically transported to a colony. 

- People are required for research, ships, and other designated human-dependent roles. 

- A house automatically provides enough basic food and resources for residents to survive even when the colony is abandoned. 

- Population grows naturally when housing, food, and life-support conditions are sufficient. 

- Survival is distinct from productivity: residents in an unactivated or abandoned colony cannot work. 

- The player must visit a colony to establish its work systems in person. 

- If a colony is abandoned, human-dependent activities remain inactive until the player returns and re-establishes operations. 

##### **Design distinction** 

Robots allow industry to operate without a local human workforce. People unlock research, crewed operations, specialized work, and population growth. Housing supports survival but does not automatically make a colony productive. 

Working design plan • Concept and systems specification 

**BEYOND THE STARS  |  GAME DESIGN DOCUMENT** 

## **10. Research and technology** 

#### **Research mechanics** 

- Staffed research facilities generate research points over time. 

- Research points enter a civilization-wide shared pool and do not need to be shipped. 

- People and research facilities must physically exist somewhere to generate points. 

- A colony without active human work systems cannot generate human-produced research. 

- Technology unlocks are civilization-wide, but physical equipment must still be manufactured and delivered. 

- Research progresses according to simulation speed and stops when time is paused. 

#### **Branching technology tree** 

|**Branch**|**Focus**|
|---|---|
|Industrial engineering|Machine generations, belts, modules, extraction, production<br>throughput.|
|Energy|Generation, storage, power distribution, interplanetary and<br>interstellar transmission.|
|Rocketry and propulsion|Cargo ships, advanced drives, greater range, intergalactic travel.|
|Automation|Robotics, logistics scheduling, production policies, network<br>management.|
|Planetary engineering|Environmental adaptation, outposts, habitats, specialized<br>settlement infrastructure.|
|Astrophysics and exotic science|Rare phenomena, advanced energy, exotic materials, extreme-<br>distance travel.|



- Technologies have explicit prerequisites and can depend on more than one branch. 

- 

- 

- 

- Players can specialize, while major progression milestones may require cross-branch investment. 

- Players can switch research projects freely; accumulated progress is preserved. 

- Research should unlock new capabilities, not only larger numbers or higher costs. 

## **11. Time control and Instant-Warp** 

Time control is a core progression system and should be integrated into the technology tree. The player can pause, run at normal speed, accelerate through increasingly high multipliers, or use a destination-based Instant-Warp ability. 

|**Mode**|**Behavior**|
|---|---|
|Pause|Stops ordinary simulation progression.|
|Normal|Runs at standard simulation speed.|
|Fast 2×, 4×, 8×, 16×|Early and intermediate acceleration levels.|
|Fast 32×, 64×, 128×, 256×|Advanced acceleration levels.|
|Further acceleration|The progression can continue beyond 256× as research and<br>balance permit.|
|Instant-Warp|Advances simulation time until a selected scout ship or other<br>vessel reaches its destination.|



#### **Suggested time-control technology progression** 

|**Tier**|**Technology**|**Effect**|
|---|---|---|
|1|Basic Time Acceleration|Unlocks Fast Mode at 2×.|
|1|Acceleration I–III|Unlocks 4×, 8×, and 16×.|
|2|Advanced Acceleration|Unlocks 32×, 64×, 128×, and 256×.|
|3|Ultra / Hyper Acceleration|Unlocks further multipliers beyond 256×.|



Working design plan • Concept and systems specification 

###### **BEYOND THE STARS  |  GAME DESIGN DOCUMENT** 

|4|Time Compression|Unlocks very high acceleration and<br>advanced time-control options.|
|---|---|---|
|Ultimate|Instant-Warp|Select a vessel and advance simulation<br>time until it reaches its destination.|



The multiplier list and exact research costs are illustrative. They need balancing against production rates, travel durations, and interface usability. Instant-Warp should be clearly distinguished from teleportation: it advances time to the arrival event rather than making cargo appear without travel. 

## **12. Procedural universe generation** 

|**Layer**|**Generated content**|
|---|---|
|Galaxy|Shape, density, star distribution, regions, major phenomena.|
|Star system|Star type, planet count, orbital arrangement, moons, broad<br>system characteristics.|
|Planet|Size, terrain, atmosphere, resources, hazards, possible life.|
|Surface|Buildable terrain, deposit placement, obstacles, local points of<br>interest.|



- Use deterministic seeds or equivalent stable identifiers so generated locations remain consistent. 

- Persist changes: buildings, inventories, surveys, defeated threats, and other world state. 

- Generate detailed surfaces on demand rather than simulating every planet in full detail at once. 

- Ensure progression-critical resources remain attainable; rare finds may be bonuses but should not randomly block mandatory progress. 

- Make worlds strategically distinct through resource profiles, terrain, hazards, rare phenomena, and opportunities for specialized industry. 

#### **Planet categories** 

|**Category**|**Potential identity**|
|---|---|
|Rocky|Common minerals and straightforward industrial expansion.|
|Ice|Water, frozen volatiles, cold-environment engineering.|
|Volcanic|Heat-related hazards and specialized industrial opportunities.|
|Living|Biological resources and possible hostile organisms.|
|Metal-rich|High-value ores that reward extraction and cargo capacity.|
|Exotic|Rare resources and phenomena for advanced technologies.|



Categories can overlap; they should not be rigid templates. Actual resource distributions remain uncertain until adequate surveys are completed. 

## **13. Hostile life and planetary security** 

- Some planets may contain hostile organisms, discoverable through scanning, surveys, or planetary information. 

- The player is an invulnerable overhead overseer; there is no player-death mechanic. 

- Colonists and other people can be killed by threats or dangerous conditions. 

- Players choose between eliminating hostile life and establishing defenses to coexist where the world’s mechanics permit. 

- Threats should be readable, explainable, and useful as a planning challenge rather than a mandatory combat encounter on every world. 

- Surveying, suitable equipment, protected areas, and colony placement should affect risk. 

Working design plan • Concept and systems specification 

**BEYOND THE STARS  |  GAME DESIGN DOCUMENT** 

## **14. User interface and controls** 

|**Interface area**|**Information and purpose**|
|---|---|
|Planet view|Local resource bar, building tools, machine details, power<br>status, construction menus.|
|Map view (M)|System/galaxy navigation, known destinations, scan information,<br>travel selection.|
|Research|Branching tree, prerequisites, research costs, progress, project<br>selection.|
|Logistics|Ships, cargo manifests, routes, delivery status, fuel, shipment<br>priorities.|
|Colony management|Population, housing, active work systems, research staffing,<br>colony status.|
|Time controls|Pause, normal, fast multipliers, further acceleration, and<br>Instant-Warp.|



- Distinguish unknown, scanned, surveyed, colonized, and operational locations. 

- Show production rates and bottlenecks on selected machines. 

- Make power shortages and blocked conveyors obvious. 

- Show each shipment’s origin, destination, cargo, and status. 

- Keep generation messages and quotations temporary rather than permanent HUD elements. 

## **15. Visual and audio direction** 

The approved visual direction is colorful, polished sci-fi industrial construction: readable factories, vivid planets, distinct resource icons, detailed map interfaces, and clear production status. 

- Prioritize readability of belts, inserters, buildings, and machine states at normal zoom. 

- Differentiate worlds through terrain, color, atmosphere, resources, and hazards. 

- Make scale progression visible: small early factories grow into enormous industrial networks. 

- Use consistent interface language across planet, star-system, and galaxy views. 

- Use procedural variety in world silhouettes, surfaces, lighting, and environmental details. 

#### **Audio direction** 

- Distinct machine sounds that communicate operation and shutdown. 

- Clear alerts for power failures, blocked belts, and completed shipments. 

- Different ambience for planets, travel, and unusual phenomena. 

- Subtle feedback for construction and research completion. 

- Occasional humorous travel-generation messages with restrained audio so they do not become intrusive. 

## **16. Progression across six stages** 

|**Stage**|**Unlocks and focus**|**Primary challenge**|
|---|---|---|
|1. Homeworld industry|Basic extraction, smelting, belts, power,<br>machines, first spacecraft.|Create a functioning production chain<br>from limited resources.|
|2. Planetary expansion|Scouts, surveys, outposts, remote<br>extraction, cargo delivery.|Prepare expeditions and physically deliver<br>construction needs.|
|3. System-wide industry|Specialized planets, automated shipping,<br>orbital infrastructure, interplanetary<br>power.|Coordinate production and cargo across<br>independent inventories.|
|4. Interstellar expansion|Interstellar propulsion and distant<br>colonies.|Build reliable supply chains over longer<br>distances.|
|5. Galactic civilization|Large industrial regions, advanced energy<br>networks, rare resources.|Manage throughput and complexity<br>across huge networks.|



Working design plan • Concept and systems specification 

**BEYOND THE STARS  |  GAME DESIGN DOCUMENT** 

|6. Intergalactic expansion|Exotic propulsion, intergalactic travel,<br>new frontier systems.|Create new industrial possibilities without<br>makingearlier systems irrelevant.|
|---|---|---|



The endgame should introduce new capabilities and strategic problems, not merely more expensive versions of earlygame recipes. Earlier industry should remain useful as part of the dependency chain. 

## **17. Balancing principles** 

|**Risk**|**Design response**|
|---|---|
|Excessive micromanagement|Housing supports survival; research is shared; projects switch<br>freely; machines resume automatically; routine shipping can be<br>automated.<br>i|
|Logistics becomes meaningless|Keep physical inventories, finite cargo capacity, ship availability,<br>and delivery requirements.|
|Abandoned colonies feel pointless|Robotic industry can continue, while human-dependent<br>activities require active work systems.|
|Infinite deposits remove challenge|Constrain extraction throughput, power, machine capacity, and<br>downstream processing.|
|Procedural worlds feel interchangeable|Use distinct resource profiles, terrain, hazards, and strategic<br>roles.|
|Late game becomes bigger numbers only|Introduce new logistics, energy, propulsion, and industrial<br>capabilities.|
|Players lose track of resources|Provide manifests, production statistics, colony summaries, and<br>clear local inventories.|
|Hostile life feels unfair|Provide readable threats, preparation options, and defensive<br>alternatives.|
|Universe overwhelms performance|Generate detailed worlds on demand and persist their state.|



## **18. Design and validation roadmap** 

|**Milestone**|**Focus**|**Deliverable**|
|---|---|---|
|1. Finalize rules|Production economy, time, inventories,<br>colonization, research.|Consistent design specification.|
|2. Validate factory loop|Extraction, recipes, belts, inserters,<br>machine upgrades, local power.|Satisfying small-scale factory design.|
|3. Validate physical expansion|Ships, manifests, scouting, surveys,<br>outposts.|Trip from homeworld to a supplied<br>operational colony.|
|4. Validate system-scale management|Multiple colonies, research, shipping<br>automation, power transmission.|Network of specialized planets with<br>independent inventories.|
|5. Validate procedural scale|Stable generated systems, galaxy<br>navigation, advanced propulsion.|Scalable progression without repetitive<br>expansion.|
|6. Complete progression and polish|Technology tree, hostile life, UI, audio,<br>balance, endgame.|Coherent experience from first factory to<br>intergalactic expansion.|



Recommended priority: validate the early factory loop before fully specifying late-game exotic resources. Space expansion should multiply the value of a strong production foundation. 

## **19. Remaining design decisions** 

|**Topic**|**Open question**|
|---|---|
|Recipes and production rates|Exact costs, processing times, machine throughput, and<br>resource ratios.|
|Ship and travel model|Fuel formulas, cargo capacities, travel durations, and propulsion<br>rules.|
|Colony work-system activation|What theplayer must build/configure duringthe required on-site|



Working design plan • Concept and systems specification 

###### **BEYOND THE STARS  |  GAME DESIGN DOCUMENT** 

||visit, and how abandonment affects established systems.|
|---|---|
|Hostile organisms|Threat types, detection, defenses, elimination, and coexistence<br>mechanics.|
|Generation limits|Number of planets/systems per region and guarantees for<br>progression-critical resources.|
|Endgame objective|Unlimited expansion, a megastructure, a scientific breakthrough,<br>or another civilization-scalegoal.|
|Advanced automation|Shipping schedules, stock thresholds, production priorities, and<br>colony-level policies.|
|Time-control balancing|How very high multipliers and Instant-Warp interact with<br>production, travel, events, and pause.|



## **20. Definition of success** 

15. Build a useful factory on the home planet. 

16. Discover a resource that is difficult or impossible to obtain locally. 

17. Manufacture a spacecraft and reach the resource-bearing destination. 

18. Survey the destination and deliver the equipment required to exploit it. 

19. Use the new resource to create better machines, ships, and research capabilities. 

20. Establish specialized colonies and coordinate production through physical logistics. 

21. Automate routine operations without eliminating strategic decisions. 

22. Scale the same principles across star systems and galaxies. 

23. Unlock intergalactic travel and discover new industrial possibilities at the frontier. 

##### **Unifying principle** 

Automation within locations. Logistics between locations. Research that expands what the civilization can do. 

## **Appendix A — Sample time-control tech tree** 

This is an illustrative progression based on the current concept. Exact node names, research costs, prerequisites, and multipliers beyond the specified levels remain open to balancing. 

|**Tier**|**Example node**|**Effect**|
|---|---|---|
|1|Basic Time Acceleration|Unlocks Fast Mode 2×.|
|1|Fast Mode 4×|Doubles the previous maximum speed.|
|1|Fast Mode 8×|Higher acceleration for routine<br>operations.|
|1|Fast Mode 16×|Advanced early-game acceleration.|
|2|Fast Mode 32×|Significantly accelerates simulation time.|
|2|Fast Mode 64×|Supports longer waits and travel.|
|2|Fast Mode 128×|Very high simulation acceleration.|
|2|Fast Mode 256×|High-tier fast mode.|
|3+|Further acceleration|Continue beyond 256× through additional<br>technologies.|
|Ultimate|Instant-Warp|Advance time until a selected ship or<br>scout reaches its destination.|



The tree can branch into propulsion, energy, and automation dependencies, but Instant-Warp should remain a timecontrol ability, not a cargo teleportation mechanic. 

## **Appendix B — Glossary** 

**<mark>Meaning</mark>** 

**<mark>Term</mark>** 

Working design plan • Concept and systems specification 

###### **BEYOND THE STARS  |  GAME DESIGN DOCUMENT** 

|Outpost|A newly established location with limited infrastructure and<br>purpose.|
|---|---|
|Colony activation|The player’s required on-site establishment of work systems for<br>human-dependent activities.|
|Local inventory|Resources stored on a particular planet, ship, or storage<br>location.|
|Shared research pool|Civilization-wide research pointsgenerated by staffed facilities.|
|Instant-Warp|A time-advance command that waits for a selected vessel’s<br>destination arrival.|
|Procedural generation|Rule- and seed-driven creation of star systems, planets, and<br>surfaces.|
|Throughput|The quantity of material a machine or transport system can<br>process per unit of time.|



Working design plan • Concept and systems specification
