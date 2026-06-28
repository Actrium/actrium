import SwiftUI
import Combine

// MARK: - Sequence Diagram Data Models

/// Participant in the sequence diagram (Guest, Host, or remote service)
struct SequenceParticipant: Identifiable, Hashable {
    let id: String
    let name: String
    let isLocal: Bool  // Guest/Host are local, remote services are not

    static let guest = SequenceParticipant(id: "guest", name: "Guest", isLocal: true)
    static let host = SequenceParticipant(id: "host", name: "Host", isLocal: true)

    static func remoteService(host: String) -> SequenceParticipant {
        // Shorten common domains
        let shortName = host
            .replacingOccurrences(of: "www.", with: "")
            .replacingOccurrences(of: ".com", with: "")
            .replacingOccurrences(of: ".io", with: "")
            .replacingOccurrences(of: ".org", with: "")
        return SequenceParticipant(id: host, name: shortName, isLocal: false)
    }
}

/// Event type in the sequence diagram
enum SequenceEventType {
    case request   // Arrow going right (to service)
    case response  // Arrow going left (from service)
    case timeGap   // ... marker for time gaps
}

/// A single event in the sequence diagram
struct SequenceEvent: Identifiable {
    let id = UUID()
    let timestamp: Date
    let type: SequenceEventType
    let from: SequenceParticipant
    let to: SequenceParticipant
    let requestNumber: Int  // For pairing request/response
    let request: DevToolsNetworkRequest?
    let label: String

    // For time gap events
    var gapDuration: TimeInterval = 0
}

// MARK: - Sequence Diagram View Model

@MainActor
class NetworkSequenceViewModel: ObservableObject {
    @Published var participants: [SequenceParticipant] = []
    @Published var events: [SequenceEvent] = []
    @Published var selectedRequestId: UUID?

    private let timeGapThreshold: TimeInterval = 2.0  // Compress gaps > 2 seconds

    /// Build sequence diagram from network requests
    func buildSequence(from requests: [DevToolsNetworkRequest]) {
        var allParticipants: [String: SequenceParticipant] = [
            "guest": .guest,
            "host": .host
        ]
        var allEvents: [SequenceEvent] = []

        // Sort requests by start time
        let sortedRequests = requests.sorted { $0.startTime < $1.startTime }

        for (index, request) in sortedRequests.enumerated() {
            let requestNumber = index + 1

            // Extract remote service from URL
            let remoteHost = URL(string: request.url)?.host ?? "unknown"
            if allParticipants[remoteHost] == nil {
                allParticipants[remoteHost] = .remoteService(host: remoteHost)
            }
            let remoteParticipant = allParticipants[remoteHost]!

            // Request event: Host -> Remote Service
            let requestEvent = SequenceEvent(
                timestamp: request.startTime,
                type: .request,
                from: .host,
                to: remoteParticipant,
                requestNumber: requestNumber,
                request: request,
                label: "\(request.method) \(shortenPath(request.displayPathWithQuery))"
            )
            allEvents.append(requestEvent)

            // Response event (if completed)
            if let endTime = request.endTime {
                let responseEvent = SequenceEvent(
                    timestamp: endTime,
                    type: .response,
                    from: remoteParticipant,
                    to: .host,
                    requestNumber: requestNumber,
                    request: request,
                    label: request.statusText
                )
                allEvents.append(responseEvent)
            }
        }

        // Sort all events by timestamp
        allEvents.sort { $0.timestamp < $1.timestamp }

        // Insert time gap markers
        var eventsWithGaps: [SequenceEvent] = []
        for (index, event) in allEvents.enumerated() {
            if index > 0 {
                let prevTime = allEvents[index - 1].timestamp
                let gap = event.timestamp.timeIntervalSince(prevTime)
                if gap > timeGapThreshold {
                    var gapEvent = SequenceEvent(
                        timestamp: prevTime.addingTimeInterval(gap / 2),
                        type: .timeGap,
                        from: .guest,
                        to: .guest,
                        requestNumber: 0,
                        request: nil,
                        label: "..."
                    )
                    gapEvent.gapDuration = gap
                    eventsWithGaps.append(gapEvent)
                }
            }
            eventsWithGaps.append(event)
        }

        // Order participants: Guest, Host, then remote services alphabetically
        let orderedParticipants = [SequenceParticipant.guest, SequenceParticipant.host] +
            allParticipants.values
                .filter { !$0.isLocal }
                .sorted { $0.name < $1.name }

        self.participants = orderedParticipants
        self.events = eventsWithGaps
    }

    private func shortenPath(_ path: String) -> String {
        if path.count > 30 {
            return String(path.prefix(27)) + "..."
        }
        return path
    }
}

// MARK: - Main Network Sequence View

struct NetworkSequenceView: View {
    @ObservedObject var devTools: DevToolsManager
    @ObservedObject var viewModel: BrowserViewModel
    @StateObject private var sequenceVM = NetworkSequenceViewModel()

    @State private var typeFilter: DevToolsNetworkRequestType? = nil  // nil = all types
    @State private var searchText: String = ""

    private var currentTabIdString: String? {
        viewModel.currentTabId?.uuidString
    }

    private var filteredRequests: [DevToolsNetworkRequest] {
        guard let currentTabIdString else { return [] }

        return devTools.networkRequests
            .filter { req in
                if req.tabId != currentTabIdString { return false }

                // Type filter
                if let typeFilter, req.requestType != typeFilter { return false }

                // Search filter
                if !searchText.isEmpty {
                    let q = searchText.lowercased()
                    let matchesUrl = req.url.lowercased().contains(q)
                    let matchesMethod = req.method.lowercased().contains(q)
                    if !(matchesUrl || matchesMethod) { return false }
                }
                return true
            }
    }

    private var selectedRequest: DevToolsNetworkRequest? {
        guard let selectedId = sequenceVM.selectedRequestId else { return nil }
        return devTools.networkRequests.first { $0.id == selectedId }
    }

    var body: some View {
        VStack(spacing: 0) {
            // Filter bar
            filterBar

            Divider()

            // Content
            if devTools.networkRequests.isEmpty {
                emptyState
            } else {
                HSplitView {
                    // Left: Sequence diagram
                    SequenceDiagramView(
                        viewModel: sequenceVM,
                        participants: sequenceVM.participants,
                        events: sequenceVM.events
                    )
                    .frame(minWidth: 400)

                    // Right: Request details
                    RequestDetailsView(request: selectedRequest)
                        .frame(minWidth: 250, idealWidth: 350)
                }
            }
        }
        .background(Color(nsColor: .textBackgroundColor))
        .onChange(of: devTools.networkRequestsVersion) { _, _ in
            // Rebuild when any network request changes
            sequenceVM.buildSequence(from: filteredRequests)
        }
        .onAppear {
            sequenceVM.buildSequence(from: filteredRequests)
        }
    }

    private var filterBar: some View {
        HStack(spacing: 8) {
            // Type filter buttons
            HStack(spacing: 2) {
                typeFilterButton(nil, label: "All")
                ForEach(DevToolsNetworkRequestType.allCases) { type in
                    typeFilterButton(type, label: type.rawValue)
                }
            }

            // Search
            HStack(spacing: 4) {
                Image(systemName: "magnifyingglass")
                    .font(.system(size: 10))
                    .foregroundColor(.secondary)
                TextField("Filter", text: $searchText)
                    .textFieldStyle(.plain)
                    .font(.system(size: 11))
            }
            .padding(.horizontal, 6)
            .padding(.vertical, 3)
            .background(Color(nsColor: .textBackgroundColor))
            .cornerRadius(4)
            .frame(maxWidth: 140)

            Spacer()

            Text("\(filteredRequests.count) requests")
                .font(.system(size: 10))
                .foregroundColor(Color(nsColor: .tertiaryLabelColor))

            Button(action: { devTools.clearNetwork() }) {
                Image(systemName: "trash")
                    .font(.system(size: 11))
            }
            .buttonStyle(.borderless)
            .help("Clear network")
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .background(Color(nsColor: .controlBackgroundColor))
    }

    private func typeFilterButton(_ type: DevToolsNetworkRequestType?, label: String) -> some View {
        Button(action: { typeFilter = type }) {
            Text(label)
                .font(.system(size: 10))
                .padding(.horizontal, 6)
                .padding(.vertical, 2)
                .background(typeFilter == type ? Color.accentColor : Color.clear)
                .foregroundColor(typeFilter == type ? .white : .secondary)
                .cornerRadius(3)
        }
        .buttonStyle(.plain)
    }

    private var emptyState: some View {
        VStack(spacing: 8) {
            Image(systemName: "network")
                .font(.system(size: 24))
                .foregroundColor(.secondary)
            Text("No network requests")
                .font(.system(size: 12))
                .foregroundColor(.secondary)
            Text("Requests will appear when Guest triggers fetch")
                .font(.system(size: 11))
                .foregroundColor(.secondary.opacity(0.7))
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

// MARK: - Sequence Diagram View

struct SequenceDiagramView: View {
    @ObservedObject var viewModel: NetworkSequenceViewModel
    let participants: [SequenceParticipant]
    let events: [SequenceEvent]

    private let participantWidth: CGFloat = 100
    private let rowHeight: CGFloat = 28
    private let timestampWidth: CGFloat = 70

    var body: some View {
        VStack(spacing: 0) {
            // Header with participants
            participantHeader

            Divider()

            // Events
            ScrollView {
                LazyVStack(spacing: 0) {
                    ForEach(events) { event in
                        SequenceEventRow(
                            event: event,
                            participants: participants,
                            participantWidth: participantWidth,
                            timestampWidth: timestampWidth,
                            isSelected: viewModel.selectedRequestId == event.request?.id,
                            onTap: {
                                if let request = event.request {
                                    viewModel.selectedRequestId = request.id
                                }
                            }
                        )
                        .frame(height: event.type == .timeGap ? 20 : rowHeight)
                    }
                }
                .padding(.vertical, 4)
            }
        }
        .background(Color(nsColor: .textBackgroundColor))
    }

    private var participantHeader: some View {
        HStack(spacing: 0) {
            // Timestamp column header
            Text("Time")
                .font(.system(size: 10, weight: .semibold))
                .foregroundColor(.secondary)
                .frame(width: timestampWidth)

            // Participant headers
            ForEach(participants) { participant in
                Text(participant.name)
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundColor(participant.isLocal ? .primary : .orange)
                    .frame(width: participantWidth)
                    .lineLimit(1)
            }

            Spacer()
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        .background(Color(nsColor: .controlBackgroundColor))
    }
}

// MARK: - Sequence Event Row

struct SequenceEventRow: View {
    let event: SequenceEvent
    let participants: [SequenceParticipant]
    let participantWidth: CGFloat
    let timestampWidth: CGFloat
    let isSelected: Bool
    let onTap: () -> Void

    private let colors: [Color] = [.blue, .green, .orange, .purple, .pink, .cyan, .yellow]

    private var eventColor: Color {
        colors[(event.requestNumber - 1) % colors.count]
    }

    var body: some View {
        HStack(spacing: 0) {
            // Timestamp (show gap duration for time gap events)
            if event.type == .timeGap {
                Text(formatGapDuration(event.gapDuration))
                    .font(.system(size: 9, design: .monospaced))
                    .foregroundColor(.secondary.opacity(0.6))
                    .frame(width: timestampWidth, alignment: .trailing)
                    .padding(.trailing, 8)

                // Empty row
                timeGapRow
            } else {
                Text(formatTime(event.timestamp))
                    .font(.system(size: 9, design: .monospaced))
                    .foregroundColor(.secondary)
                    .frame(width: timestampWidth, alignment: .trailing)
                    .padding(.trailing, 8)

                // Normal event row with arrow
                eventRow
            }

            Spacer()
        }
        .padding(.horizontal, 8)
        .background(isSelected ? Color.accentColor.opacity(0.2) : Color.clear)
        .contentShape(Rectangle())
        .onTapGesture { onTap() }
    }

    private var timeGapRow: some View {
        // Empty row - gap duration shown in timestamp column
        Spacer()
    }

    private func formatGapDuration(_ seconds: TimeInterval) -> String {
        if seconds < 60 {
            return "+\(Int(seconds))s"
        } else if seconds < 3600 {
            let mins = Int(seconds / 60)
            let secs = Int(seconds.truncatingRemainder(dividingBy: 60))
            return "+\(mins)m\(secs)s"
        } else {
            let hours = Int(seconds / 3600)
            let mins = Int((seconds.truncatingRemainder(dividingBy: 3600)) / 60)
            return "+\(hours)h\(mins)m"
        }
    }

    private var eventRow: some View {
        HStack(spacing: 0) {
            // Draw lifelines and arrow
            let fromIndex = participants.firstIndex(of: event.from) ?? 0
            let toIndex = participants.firstIndex(of: event.to) ?? 0

            ForEach(Array(participants.enumerated()), id: \.element.id) { index, participant in
                ZStack {
                    // Lifeline
                    Rectangle()
                        .fill(Color.secondary.opacity(0.3))
                        .frame(width: 1)

                    // Arrow components
                    if index == fromIndex || index == toIndex {
                        // Dot at endpoints
                        Circle()
                            .fill(eventColor)
                            .frame(width: 6, height: 6)
                    }
                }
                .frame(width: participantWidth)
            }

            // Label with request number
            HStack(spacing: 4) {
                Text("#\(event.requestNumber)")
                    .font(.system(size: 9, weight: .bold, design: .monospaced))
                    .foregroundColor(eventColor)

                Text(event.label)
                    .font(.system(size: 10))
                    .foregroundColor(event.type == .response ? .green : .primary)
                    .lineLimit(1)
            }
            .padding(.leading, 8)
        }
        .overlay(
            // Draw arrow line
            GeometryReader { geo in
                let fromX = timestampWidth + CGFloat(participants.firstIndex(of: event.from) ?? 0) * participantWidth + participantWidth / 2
                let toX = timestampWidth + CGFloat(participants.firstIndex(of: event.to) ?? 0) * participantWidth + participantWidth / 2
                let adjustedFromX = fromX - timestampWidth - 8
                let adjustedToX = toX - timestampWidth - 8

                Path { path in
                    path.move(to: CGPoint(x: adjustedFromX, y: geo.size.height / 2))
                    path.addLine(to: CGPoint(x: adjustedToX, y: geo.size.height / 2))
                }
                .stroke(eventColor, lineWidth: 1.5)

                // Arrow head
                let arrowDirection: CGFloat = adjustedToX > adjustedFromX ? -1 : 1
                Path { path in
                    let tipX = adjustedToX
                    let tipY = geo.size.height / 2
                    path.move(to: CGPoint(x: tipX, y: tipY))
                    path.addLine(to: CGPoint(x: tipX + arrowDirection * 6, y: tipY - 4))
                    path.move(to: CGPoint(x: tipX, y: tipY))
                    path.addLine(to: CGPoint(x: tipX + arrowDirection * 6, y: tipY + 4))
                }
                .stroke(eventColor, lineWidth: 1.5)
            }
        )
    }

    private func formatTime(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm:ss.SS"
        return formatter.string(from: date)
    }
}

// MARK: - Request Details View

struct RequestDetailsView: View {
    let request: DevToolsNetworkRequest?

    @State private var selectedTab: DetailTab = .headers

    enum DetailTab: String, CaseIterable {
        case headers = "Headers"
        case request = "Request"
        case response = "Response"
    }

    var body: some View {
        VStack(spacing: 0) {
            if let request {
                // Header
                VStack(alignment: .leading, spacing: 4) {
                    HStack {
                        Text(request.method)
                            .font(.system(size: 11, weight: .semibold, design: .monospaced))
                            .foregroundColor(.blue)
                        Text(request.displayPathWithQuery)
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundColor(.primary)
                            .lineLimit(1)
                    }

                    HStack(spacing: 8) {
                        Text(request.displayHost)
                            .font(.system(size: 10))
                            .foregroundColor(.secondary)

                        Text(request.statusText)
                            .font(.system(size: 10, weight: .medium))
                            .foregroundColor(request.statusColor)

                        if let duration = request.duration {
                            Text(String(format: "%.0fms", duration * 1000))
                                .font(.system(size: 10))
                                .foregroundColor(.secondary)
                        }
                    }
                }
                .padding(8)
                .background(Color(nsColor: .controlBackgroundColor))

                // Tab picker - left aligned
                HStack(spacing: 0) {
                    ForEach(DetailTab.allCases, id: \.self) { tab in
                        Button(action: { selectedTab = tab }) {
                            Text(tab.rawValue)
                                .font(.system(size: 11))
                                .padding(.horizontal, 12)
                                .padding(.vertical, 4)
                                .background(selectedTab == tab ? Color.accentColor : Color.clear)
                                .foregroundColor(selectedTab == tab ? .white : .secondary)
                                .cornerRadius(4)
                        }
                        .buttonStyle(.plain)
                    }
                    Spacer()
                }
                .padding(8)

                Divider()

                // Content
                ScrollView {
                    VStack(alignment: .leading, spacing: 0) {
                        switch selectedTab {
                        case .headers:
                            headersContent(request)
                        case .request:
                            bodyContent(request.requestBody, title: "Request Body")
                        case .response:
                            bodyContent(request.responseBody, title: "Response Body")
                        }
                    }
                    .padding(8)
                }
            } else {
                VStack(spacing: 8) {
                    Image(systemName: "arrow.left.circle")
                        .font(.system(size: 24))
                        .foregroundColor(.secondary)
                    Text("Select a request")
                        .font(.system(size: 12))
                        .foregroundColor(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .background(Color(nsColor: .textBackgroundColor))
    }

    @ViewBuilder
    private func headersContent(_ request: DevToolsNetworkRequest) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            // Request headers
            if let headers = request.requestHeaders, !headers.isEmpty {
                headerSection("Request Headers", headers: headers)
            }

            // Response headers
            if let headers = request.responseHeaders, !headers.isEmpty {
                headerSection("Response Headers", headers: headers)
            }

            if request.requestHeaders == nil && request.responseHeaders == nil {
                Text("No headers available")
                    .font(.system(size: 11))
                    .foregroundColor(.secondary)
            }
        }
    }

    @ViewBuilder
    private func headerSection(_ title: String, headers: [String: String]) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(.system(size: 10, weight: .semibold))
                .foregroundColor(.secondary)

            ForEach(headers.sorted(by: { $0.key < $1.key }), id: \.key) { key, value in
                HStack(alignment: .top, spacing: 8) {
                    Text(key + ":")
                        .font(.system(size: 10, design: .monospaced))
                        .foregroundColor(.blue)
                        .textSelection(.enabled)
                    Text(value)
                        .font(.system(size: 10, design: .monospaced))
                        .foregroundColor(.primary)
                        .textSelection(.enabled)
                }
            }
        }
        .padding(8)
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.5))
        .cornerRadius(4)
    }

    @ViewBuilder
    private func bodyContent(_ body: String?, title: String) -> some View {
        if let body, !body.isEmpty {
            VStack(alignment: .leading, spacing: 4) {
                Text(title)
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundColor(.secondary)

                Text(formatBody(body))
                    .font(.system(size: 10, design: .monospaced))
                    .foregroundColor(.primary)
                    .textSelection(.enabled)
                    .padding(8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(Color(nsColor: .controlBackgroundColor).opacity(0.5))
                    .cornerRadius(4)
            }
        } else {
            Text("No body")
                .font(.system(size: 11))
                .foregroundColor(.secondary)
        }
    }

    private func formatBody(_ body: String) -> String {
        // Try to pretty print JSON
        if let data = body.data(using: .utf8),
           let json = try? JSONSerialization.jsonObject(with: data),
           let prettyData = try? JSONSerialization.data(withJSONObject: json, options: .prettyPrinted),
           let prettyString = String(data: prettyData, encoding: .utf8) {
            return prettyString
        }
        return body
    }
}
