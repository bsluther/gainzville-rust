import SwiftUI

struct LogView: View {
    @EnvironmentObject var forestVM: ForestViewModel

    var body: some View {
        Group {
            if forestVM.roots.isEmpty {
                ContentUnavailableView(
                    "No Entries",
                    systemImage: "list.bullet.rectangle",
                    description: Text("Entries you log will appear here.")
                )
            } else {
                ScrollView {
                    VStack(spacing: GvSpacing.md) {
                        ForEach(forestVM.roots, id: \.id) { entry in
                            EntryView(entry: entry)
                        }
                    }
                    .padding(.horizontal, GvSpacing.xl)
                    .padding(.vertical, GvSpacing.md)
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .navigationTitle("Log")
        .background(Color.gvBackground)
    }
}

#Preview {
    NavigationStack {
        LogView()
            .environmentObject(ForestViewModel())
    }
}
