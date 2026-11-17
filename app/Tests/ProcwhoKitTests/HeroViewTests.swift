import Testing
@testable import ProcwhoKit

struct HeroViewTests {
    @Test func pressureReplacesLoadUnlessMissing() {
        var status = StatusDoc(system: SystemInfo(ncpu: 1, load1: 100))
        status.system?.memoryPressure = "normal"
        #expect(Presentation.machineAttention(status) == nil)
        status.system?.memoryPressure = "warning"
        #expect(Presentation.machineAttention(status) == "Memory pressure is high")
        #expect(Presentation.pressureState("warning") == .attention)
        #expect(Presentation.pressureState("critical") == .critical)
        #expect(Presentation.pressureState("normal") == .normal)
        status.system?.memoryPressure = nil
        #expect(Presentation.machineAttention(status)?.hasPrefix("Load") == true)
    }
    @Test func cycleFallbackAndExclusiveTotals() {
        #expect(HeroView.associated.next == .agents)
        #expect(HeroView.agents.next == .memoryUsed)
        #expect(HeroView.memoryUsed.next == .associated)
        var status = StatusDoc(system: SystemInfo(memTotalBytes: 100, memUsedBytes: 60))
        #expect(HeroView.associated.effective(for: status) == .memoryUsed)
        #expect(HeroView.memoryUsed.next(for: status) == .agents)
        #expect(HeroView.associated.next(for: status) == .agents)
        #expect(Presentation.heroMemory(status) == 60)
        #expect(Presentation.heroMemory(status, view: .agents) == 0)
        var agent = Workload(key: "s", name: "Session", kind: "agent-session", footprintBytes: 10)
        agent.agentLink = AgentLink(relation: "agent", via: "agent workload", confidence: "high")
        var app = Workload(key: "a", name: "Host", kind: "app", footprintBytes: 20)
        app.agentLink = AgentLink(relation: "agent_host_app", via: "nested agent session", confidence: "high")
        var detached = Workload(key: "d", name: "Tool", kind: "detached", footprintBytes: 30)
        detached.agentLink = AgentLink(relation: "launched_by_agent", via: "responsible app: Host", confidence: "medium")
        status.workloads = [agent, app, detached]
        #expect(Presentation.heroMemory(status) == 60)
        #expect(Presentation.heroMemory(status, view: .agents) == 10)
        #expect(Presentation.mediumAssociatedMemory(status) == 30)
        #expect(Presentation.heroHelp(status, view: .associated).contains("1 host app"))
        let apps = CompositionSegment(id: "apps", title: "Apps", bytes: 80, count: 2)
        #expect(Presentation.highlightedShare(apps, status: status, view: .associated) == 0.25)
        #expect(Presentation.highlightedShare(apps, status: status, view: .agents) == 0)
        #expect(Presentation.highlightedShare(apps, status: status, view: .memoryUsed) == 1)
        let agents = CompositionSegment(id: "agents", title: "Agents", bytes: 10, count: 1)
        #expect(Presentation.highlightedShare(agents, status: status, view: .agents) == 1)
    }
}
