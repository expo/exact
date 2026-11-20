/// Session side effects retain their application phases without rescanning
/// a fill's view ops for each kind of output.
struct SessionBatchOps {
    var routers: [BatchOp] = []
    var titles: [BatchOp] = []
    var activities: [BatchOp] = []
    var surfaces: [BatchOp] = []
    var audio: [BatchOp] = []
    var authentication: [BatchOp] = []
    var reorders: [BatchOp] = []
    var roles: BatchOp?
    /// `startViewTransition(seconds)`'s duration, when the batch names it.
    var viewTransition: Double?

    init(_ ops: some Sequence<BatchOp>) {
        for op in ops {
            switch op.op {
            case .router: routers.append(op)
            case .title: titles.append(op)
            case .activity: activities.append(op)
            case .surfaceWork: surfaces.append(op)
            case .sound, .midi: audio.append(op)
            case .auth, .presence: authentication.append(op)
            case .reorder: reorders.append(op)
            case .roles: roles = op
            case .command where op.payload["name"] as? String == "startViewTransition" && viewTransition == nil:
                let seconds = (op.payload["args"] as? [Any])?.first as? Double ?? 0.25
                viewTransition = seconds.isFinite && seconds > 0 ? seconds : 0.25
            default: break
            }
        }
    }
}
