/-
Type Tour's route table names its six screens and the generated not-found
screen. Direct Display and conversation links include their parent screens.
-/
import Apps.TypeTour
open Contract

namespace TypeTour

def screens : List String := ["lock", "home", "messages", "thread", "settings", "display", "notfound"]

/-- Every declared route names one of the phone's screens. -/
theorem route_names : ∀ r ∈ typeTour.routes, r.name ∈ screens := by decide

/-- Display's Settings button has a Settings visit to return to after a direct launch. -/
theorem display_parents :
    (Route.chain typeTour.routes "/settings/display").map (·.name) =
      ["lock", "home", "settings", "display"] := by decide +kernel

/-- A directly opened conversation has the Messages list beneath it. -/
theorem thread_parents :
    (Route.chain typeTour.routes "/messages/nico").map (·.name) =
      ["lock", "home", "messages", "thread"] := by decide +kernel

end TypeTour
