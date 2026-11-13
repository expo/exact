// Engine-independent JSI adapter; no Hermes ownership, loader or event loop.
// @ref LLP 0067#3-the-check — captured authority, one Rust boundary
#include "../../include/ibex2_jsi.h"
#if __has_include(<hermes/BCGen/HBC/BytecodeFileFormat.h>)
#include <hermes/BCGen/HBC/BytecodeFileFormat.h>
#define IBEX2_HAS_HERMES_BYTECODE_FILE_FORMAT 1
#endif
#include <jsi/instrumentation.h>
#include <cstring>
#include <cmath>
#include <limits>
#include <unordered_map>
#include <unordered_set>
#include <stdexcept>

extern "C" int ibex2_host_call(const void*, uint32_t, const Ibex2AbiValue*, size_t, Ibex2AbiValue*);
extern "C" void ibex2_host_release(Ibex2AbiValue*);
extern "C" int ibex2_async_begin(const void*, const void*, uint32_t, const Ibex2AbiValue*, size_t, uint64_t);
extern "C" int ibex2_take_task(const void*, int*, unsigned long long*, Ibex2AbiValue*, int*);
extern "C" const void* ibex2_adapter_begin_drive(const void*);
extern "C" void ibex2_adapter_end_drive(const void*);
extern "C" int ibex2_adapter_admit_due_timers(const void*);
extern "C" const void* ibex2_grants_retain(const void*);
extern "C" void ibex2_grants_destroy(const void*);
extern "C" const void* ibex2_bindings_state(const Ibex2Bindings*);
extern "C" const void* ibex2_bindings_grants(const Ibex2Bindings*);
extern "C" void* ibex2_sqlite_owner_create(const void*, double, int);
extern "C" void ibex2_sqlite_owner_destroy(void*);
extern "C" void* ibex2_response_owner_create(const void*, double);
extern "C" void ibex2_response_owner_destroy(void*);
extern "C" void* ibex2_headers_owner_create(const void*, double);
extern "C" void ibex2_headers_owner_destroy(void*);
extern "C" void* ibex2_crypto_key_owner_create(const void*, double);
extern "C" void ibex2_crypto_key_owner_destroy(void*);
extern "C" int ibex2_response_field(const void*, double, uint32_t,
                                    const Ibex2AbiValue*, Ibex2AbiValue*);
extern "C" size_t ibex2_grants_env_count(const void*);
extern "C" int ibex2_grants_env_at(const void*, size_t, char**, char**);
extern "C" void ibex2_string_free(char*);
extern "C" void ibex2_report_uncaught(const char*);
extern "C" void* ibex2_subscription_create(const void*, uint64_t*);
extern "C" void ibex2_subscription_destroy(void*);
extern "C" int ibex2_websocket_supported();
extern "C" uint64_t ibex2_websocket_open(const void*, const void*, uint64_t,
    const uint8_t*, size_t, const uint8_t*, size_t, size_t, char**);
extern "C" void* ibex2_websocket_owner_create(const void*, uint64_t);
extern "C" void ibex2_websocket_owner_destroy(void*);
extern "C" int ibex2_websocket_send(const void*, uint64_t, int,
    const uint8_t*, size_t, char**);
extern "C" int ibex2_websocket_close(const void*, uint64_t, int,
    const uint8_t*, size_t, char**);
extern "C" int ibex2_websocket_ready_state(const void*, uint64_t);
extern "C" size_t ibex2_websocket_buffered_amount(const void*, uint64_t);

#if defined(IBEX2_JSI_HAS_INTL)
namespace ibex2::intl_number_format {
void install(facebook::jsi::Runtime&,
             std::shared_ptr<ibex2::jsi_adapter::Lifetime>);
}
namespace ibex2::intl_case {
void install(facebook::jsi::Runtime&,
             std::shared_ptr<ibex2::jsi_adapter::Lifetime>);
}
namespace ibex2::intl_datetime {
std::vector<facebook::jsi::Value> factory_arguments(facebook::jsi::Runtime&,
    std::shared_ptr<ibex2::jsi_adapter::Lifetime>);
}
#if defined(_WIN32)
extern "C" int ibex2_intl_os_icu_available();
#endif
#endif

namespace ibex2::jsi_adapter {
const void* Lifetime::require(jsi::Runtime& rt) const {
  if (!alive_ || state_ == nullptr)
    throw jsi::JSError(rt, "Ibex2 bindings are detached");
  return state_;
}

#include "install_values.h"

#include "install_integrity.h"

enum class InstallStatus { Fresh, Installed, Spent };

struct WebSocketOwner final : jsi::NativeState {
  void* connection;
  void* subscription;
  WebSocketOwner(void* connection_value, void* subscription_value)
      : connection(connection_value), subscription(subscription_value) {}
  ~WebSocketOwner() override {
    unsubscribe_events();
    if (connection != nullptr) {
      ibex2_websocket_owner_destroy(connection);
      connection = nullptr;
    }
  }
  void unsubscribe_events() {
    if (subscription != nullptr) {
      ibex2_subscription_destroy(subscription);
      subscription = nullptr;
    }
  }
};

struct Adapter::State {
  struct BootstrapOutput {
    std::string kind;
    std::string name;
    std::vector<std::pair<std::string, jsi::Value>> identities;
  };
  struct Pending { jsi::Function resolve; jsi::Function reject; };
  struct ImagePromiseOwner final : jsi::NativeState {
    const uint64_t task_id;
    const std::weak_ptr<State> owner;
    ImagePromiseOwner(uint64_t id, const std::shared_ptr<State>& state)
        : task_id(id), owner(state) {}
  };
  struct EventSubscription {
    void* rust;
    jsi::Function callback;
    uint64_t websocket = 0;
    std::shared_ptr<jsi::WeakObject> weak_owner;
    jsi::Value strong_owner;
    bool listener_keepalive = false;
    std::weak_ptr<WebSocketOwner> native_owner;
  };
  const void* queue;
  uint64_t next_task_id = 1;
  std::unordered_map<uint64_t, Pending> pending;
  std::unordered_map<uint64_t, EventSubscription> subscriptions;
  bool alive = true;
  InstallStatus install_status = InstallStatus::Fresh;
  Groups groups = 0;
  uint32_t bytecode_version;
  std::shared_ptr<Lifetime> lifetime;
  jsi::Value timer_dispatch;
  jsi::Value fetch_factory;
  jsi::Value websocket_factory;
  jsi::Value blob_helpers;
  jsi::Value sqlite_factory;
  jsi::Value event_reporter;
  jsi::Value trusted_event_dispatch;
  jsi::Value event_listener_query;
  jsi::Value event_listener_change_hook;
  jsi::Value rejection_unhandled;
  jsi::Value rejection_handled;
  // `Object.isFrozen` as it was at construction; a jsi::Value so detach() can
  // release it with the other roots.
  jsi::Value intrinsic_frozen;
  // %Object.prototype%, %Function.prototype%, and %Array.prototype% as they were
  // when the adapter was constructed, before any bootstrap ran. A bootstrap can
  // replace the globals that name them, but not these identities, so capture
  // can tell whether harden.js has already run.
  jsi::Value freeze_witnesses;
  std::unique_ptr<Integrity> integrity;
  bool intrinsic_snapshot_deferred = false;
  bool hardened = false;
  // Trusted-bootstrap globals and the identities published under them (the
  // object first, then each member), kept so the harden guard can prove that
  // none of them is still reachable from what harden.js freezes.
  std::vector<BootstrapOutput> bootstrap_outputs;
  std::unique_ptr<Reachability> reachability;
  State(jsi::Runtime& rt, const void* value, uint32_t version,
        std::shared_ptr<Lifetime> lifetime_value)
      : queue(value), bytecode_version(version),
        lifetime(std::move(lifetime_value)),
        intrinsic_frozen(rt.global().getPropertyAsObject(rt, "Object")
                             .getPropertyAsFunction(rt, "isFrozen")),
        freeze_witnesses(jsi::Array::createWithElements(
            rt,
            rt.global().getPropertyAsObject(rt, "Object").getProperty(rt, "prototype"),
            rt.global().getPropertyAsObject(rt, "Function").getProperty(rt, "prototype"),
            rt.global().getPropertyAsObject(rt, "Array").getProperty(rt, "prototype"))),
        integrity(std::make_unique<Integrity>(rt)) {}
  const void* require(jsi::Runtime& rt) const {
    return lifetime->require(rt);
  }
};

Adapter::Adapter(jsi::Runtime& rt, const void* queue,
                 uint32_t bytecode_version)
    : runtime_(&rt),
      state_(std::make_shared<State>(
          rt, queue, bytecode_version,
          std::shared_ptr<Lifetime>(new Lifetime(queue)))) {
  if (!queue) throw std::invalid_argument("Ibex2 bindings require runtime state");
}
Adapter::~Adapter() { detach(); }
std::shared_ptr<Lifetime> Adapter::lifetime() const {
  return state_->lifetime;
}
void Adapter::accept_trusted_intrinsic_property(jsi::Object object,
                                                const char* name) {
  if (!runtime_)
    throw std::logic_error("Ibex2 bindings are detached");
  if (!state_->integrity)
    throw std::logic_error(
        "the intrinsic snapshot is deferred; capture the complete baseline instead");
  state_->integrity->accept_property(*runtime_, object, name);
}
void Adapter::capture_intrinsics() {
  if (!runtime_) throw std::logic_error("Ibex2 bindings are detached");
  if (state_->install_status != InstallStatus::Installed)
    throw std::logic_error(
        "a deferred intrinsic snapshot can be captured only after installation");
  if (!state_->intrinsic_snapshot_deferred)
    throw std::logic_error("the intrinsic snapshot was not deferred");
  if (state_->integrity)
    throw std::logic_error("the deferred intrinsic snapshot was already captured");
  // Never consult the current globals here: a bootstrap could replace `Array`
  // with a function whose `prototype` accessor returns a fresh, unfrozen object.
  // The witnesses are the intrinsics captured at construction.
  if (state_->hardened)
    throw std::logic_error(
        "the deferred intrinsic snapshot cannot be captured after intrinsics are frozen");
  auto is_frozen = state_->intrinsic_frozen.asObject(*runtime_).asFunction(*runtime_);
  auto witnesses = state_->freeze_witnesses.asObject(*runtime_).asArray(*runtime_);
  for (size_t i = 0; i < witnesses.size(*runtime_); ++i) {
    if (is_frozen.call(*runtime_, witnesses.getValueAtIndex(*runtime_, i))
            .getBool())
      throw std::logic_error(
          "the deferred intrinsic snapshot cannot be captured after intrinsics are frozen");
  }
  state_->integrity = std::make_unique<Integrity>(*runtime_);
}
void Adapter::detach() {
  if (!state_->alive) return;
  state_->alive = false;
  for (auto& entry : state_->subscriptions) {
    if (auto owner = entry.second.native_owner.lock())
      owner->unsubscribe_events();
    else
      ibex2_subscription_destroy(entry.second.rust);
  }
  state_->subscriptions.clear();
  state_->lifetime->detach();
  state_->pending.clear();
  state_->timer_dispatch = jsi::Value::undefined();
  state_->fetch_factory = jsi::Value::undefined();
  state_->websocket_factory = jsi::Value::undefined();
  state_->blob_helpers = jsi::Value::undefined();
  state_->sqlite_factory = jsi::Value::undefined();
  state_->event_reporter = jsi::Value::undefined();
  state_->trusted_event_dispatch = jsi::Value::undefined();
  state_->event_listener_query = jsi::Value::undefined();
  state_->event_listener_change_hook = jsi::Value::undefined();
  state_->rejection_unhandled = jsi::Value::undefined();
  state_->rejection_handled = jsi::Value::undefined();
  state_->intrinsic_frozen = jsi::Value::undefined();
  state_->freeze_witnesses = jsi::Value::undefined();
  state_->integrity.reset();
  state_->bootstrap_outputs.clear();
  state_->reachability.reset();
  state_->queue = nullptr;
  runtime_ = nullptr;
}

#include "install_primitives.h"

void validate_groups(Groups groups) { validate_groups_impl(groups); }

std::vector<const char*> expected_scripts(Groups groups) {
  validate_groups_impl(groups);
  return expected_scripts_impl(groups);
}

jsi::Object Adapter::websocket_hooks(const void* grants) {
  auto& rt = *runtime_;
  auto lifetime = state_->lifetime;
  std::weak_ptr<State> weak_state = state_;
  std::shared_ptr<const void> authority(
      ibex2_grants_retain(grants), ibex2_grants_destroy);
  jsi::Object hooks(rt);
  hooks.setProperty(rt, "supported", ibex2_websocket_supported() != 0);

  hooks.setProperty(rt, "open", jsi::Function::createFromHostFunction(
      rt, jsi::PropNameID::forAscii(rt, "open"), 4,
      [lifetime, weak_state, authority](
          jsi::Runtime& r, const jsi::Value&, const jsi::Value* args,
          size_t count) -> jsi::Value {
        const void* queue = lifetime->require(r);
        auto state = weak_state.lock();
        if (!state || !state->alive)
          throw jsi::JSError(r, "Ibex2 bindings are detached");
        if (count != 4 || !args[0].isObject() || !args[1].isObject() ||
            !args[1].getObject(r).isFunction(r) || !args[2].isString() ||
            !args[3].isString())
          throw jsi::JSError(r, "WebSocket open needs an owner and callback");
        auto owner_object = args[0].getObject(r);
        auto callback = args[1].getObject(r).getFunction(r);
        std::string url = args[2].getString(r).utf8(r);
        std::string protocols = args[3].getString(r).utf8(r);
        uint64_t subscription = 0;
        void* rust = ibex2_subscription_create(queue, &subscription);
        if (rust == nullptr)
          throw jsi::JSError(r, "could not create WebSocket subscription");
        char* error = nullptr;
        uint64_t handle = ibex2_websocket_open(
            queue, authority.get(), subscription,
            reinterpret_cast<const uint8_t*>(url.data()), url.size(),
            reinterpret_cast<const uint8_t*>(protocols.data()), protocols.size(),
            16u << 20, &error);
        if (handle == 0) {
          ibex2_subscription_destroy(rust);
          websocket_result(r, 1, error, "WebSocket open failed");
        }
        auto native = std::make_shared<WebSocketOwner>(
            ibex2_websocket_owner_create(queue, handle), rust);
        if (native->connection == nullptr) {
          throw jsi::JSError(r, "could not retain WebSocket");
        }
        try {
          set_native_state_once(r, owner_object, native, "WebSocket owner");
          auto weak_owner = std::make_shared<jsi::WeakObject>(r, owner_object);
          state->subscriptions.emplace(
              subscription,
              State::EventSubscription{
                  nullptr, std::move(callback), handle, std::move(weak_owner),
                  jsi::Value::undefined(), false, native});
        } catch (...) {
          throw;
        }
        return jsi::Value(static_cast<double>(handle));
      }));

  hooks.setProperty(rt, "sendText", jsi::Function::createFromHostFunction(
      rt, jsi::PropNameID::forAscii(rt, "sendText"), 2,
      [lifetime, weak_state](jsi::Runtime& r, const jsi::Value&,
                 const jsi::Value* args, size_t count) -> jsi::Value {
        const void* queue = lifetime->require(r);
        if (count != 2 || !args[1].isString())
          throw jsi::JSError(r, "WebSocket send needs string data");
        uint64_t handle = websocket_handle(r, args[0]);
        std::string text = args[1].getString(r).utf8(r);
        char* error = nullptr;
        int status = ibex2_websocket_send(
            queue, handle, 0,
            reinterpret_cast<const uint8_t*>(text.data()), text.size(), &error);
        websocket_result(r, status, error, "WebSocket text send failed");
        if (auto state = weak_state.lock(); state && state->alive) {
          for (auto& entry : state->subscriptions) {
            if (entry.second.websocket == handle &&
                entry.second.weak_owner != nullptr) {
              auto owner = entry.second.weak_owner->lock(r);
              bool keep = entry.second.listener_keepalive ||
                  (ibex2_websocket_ready_state(queue, handle) == 1 &&
                   ibex2_websocket_buffered_amount(queue, handle) != 0);
              entry.second.strong_owner = keep && owner.isObject()
                  ? jsi::Value(r, owner) : jsi::Value::undefined();
              break;
            }
          }
        }
        return jsi::Value::undefined();
      }));

  hooks.setProperty(rt, "sendBinary", jsi::Function::createFromHostFunction(
      rt, jsi::PropNameID::forAscii(rt, "sendBinary"), 2,
      [lifetime, weak_state](jsi::Runtime& r, const jsi::Value&,
                 const jsi::Value* args, size_t count) -> jsi::Value {
        const void* queue = lifetime->require(r);
        if (count != 2) throw jsi::JSError(r, "WebSocket send needs data");
        uint64_t handle = websocket_handle(r, args[0]);
        std::vector<std::string> owned;
        auto bytes = to_abi(r, args[1], owned);
        if (bytes.tag != IBEX2_TAG_BYTES)
          throw jsi::JSError(r, "WebSocket binary data needs an ArrayBuffer or view");
        char* error = nullptr;
        int status = ibex2_websocket_send(
            queue, handle, 1, bytes.data, bytes.len, &error);
        websocket_result(r, status, error, "WebSocket binary send failed");
        if (auto state = weak_state.lock(); state && state->alive) {
          for (auto& entry : state->subscriptions) {
            if (entry.second.websocket == handle &&
                entry.second.weak_owner != nullptr) {
              auto owner = entry.second.weak_owner->lock(r);
              bool keep = entry.second.listener_keepalive ||
                  (ibex2_websocket_ready_state(queue, handle) == 1 &&
                   ibex2_websocket_buffered_amount(queue, handle) != 0);
              entry.second.strong_owner = keep && owner.isObject()
                  ? jsi::Value(r, owner) : jsi::Value::undefined();
              break;
            }
          }
        }
        return jsi::Value::undefined();
      }));

  hooks.setProperty(rt, "close", jsi::Function::createFromHostFunction(
      rt, jsi::PropNameID::forAscii(rt, "close"), 3,
      [lifetime](jsi::Runtime& r, const jsi::Value&,
                 const jsi::Value* args, size_t count) -> jsi::Value {
        const void* queue = lifetime->require(r);
        if (count != 3 || !args[2].isString())
          throw jsi::JSError(r, "WebSocket close needs its arguments");
        uint64_t handle = websocket_handle(r, args[0]);
        const int code = static_cast<int>(primitive_integer(
            r, args[1], 0.0, 4999.0, "WebSocket close code"));
        std::string reason = args[2].getString(r).utf8(r);
        char* error = nullptr;
        int status = ibex2_websocket_close(
            queue, handle, code,
            reinterpret_cast<const uint8_t*>(reason.data()), reason.size(), &error);
        websocket_result(r, status, error, "WebSocket close failed");
        return jsi::Value::undefined();
      }));

  hooks.setProperty(rt, "readyState", jsi::Function::createFromHostFunction(
      rt, jsi::PropNameID::forAscii(rt, "readyState"), 1,
      [lifetime](jsi::Runtime& r, const jsi::Value&,
                 const jsi::Value* args, size_t count) -> jsi::Value {
        const void* queue = lifetime->require(r);
        if (count != 1) throw jsi::JSError(r, "WebSocket state needs a handle");
        return jsi::Value(ibex2_websocket_ready_state(
            queue, websocket_handle(r, args[0])));
      }));

  hooks.setProperty(rt, "bufferedAmount", jsi::Function::createFromHostFunction(
      rt, jsi::PropNameID::forAscii(rt, "bufferedAmount"), 1,
      [lifetime](jsi::Runtime& r, const jsi::Value&,
                 const jsi::Value* args, size_t count) -> jsi::Value {
        const void* queue = lifetime->require(r);
        if (count != 1) throw jsi::JSError(r, "WebSocket amount needs a handle");
        return jsi::Value(static_cast<double>(ibex2_websocket_buffered_amount(
            queue, websocket_handle(r, args[0]))));
      }));

  hooks.setProperty(rt, "setKeepalive", jsi::Function::createFromHostFunction(
      rt, jsi::PropNameID::forAscii(rt, "setKeepalive"), 2,
      [lifetime, weak_state](jsi::Runtime& r, const jsi::Value&,
                            const jsi::Value* args, size_t count) -> jsi::Value {
        const void* queue = lifetime->require(r);
        if (count != 2 || !args[1].isBool())
          throw jsi::JSError(r, "WebSocket keepalive needs a handle and flag");
        uint64_t handle = websocket_handle(r, args[0]);
        auto state = weak_state.lock();
        if (!state || !state->alive)
          throw jsi::JSError(r, "Ibex2 bindings are detached");
        for (auto& entry : state->subscriptions) {
          if (entry.second.websocket != handle ||
              entry.second.weak_owner == nullptr) continue;
          auto owner = entry.second.weak_owner->lock(r);
          auto has_listener = [&](const char* type) {
            if (!owner.isObject() || !state->event_listener_query.isObject() ||
                !state->event_listener_query.getObject(r).isFunction(r))
              return false;
            auto result = state->event_listener_query.getObject(r)
                .getFunction(r).call(
                    r, owner, jsi::String::createFromAscii(r, type));
            return result.isBool() && result.getBool();
          };
          int ready = ibex2_websocket_ready_state(queue, handle);
          bool listeners = ready == 0
              ? has_listener("open") || has_listener("message") ||
                    has_listener("error") || has_listener("close")
              : ready == 1
              ? has_listener("message") || has_listener("error") ||
                    has_listener("close")
              : has_listener("error") || has_listener("close");
          entry.second.listener_keepalive = listeners;
          bool keep = entry.second.listener_keepalive ||
              (ready == 1 &&
               ibex2_websocket_buffered_amount(queue, handle) != 0);
          entry.second.strong_owner = keep && owner.isObject()
              ? jsi::Value(r, owner) : jsi::Value::undefined();
          break;
        }
        return jsi::Value::undefined();
      }));

  return hooks;
}

void Adapter::install(Groups groups, const Ibex2Bindings* bindings,
                      const CompiledScript* scripts, size_t script_count) {
  install_with(groups, bindings, scripts, script_count, InstallOptions{});
}

void Adapter::install_with(Groups groups, const Ibex2Bindings* bindings,
                           const CompiledScript* scripts, size_t script_count,
                           const InstallOptions& options) {
  if (!runtime_) throw std::logic_error("Ibex2 bindings are detached");
  auto& rt = *runtime_;
  state_->require(rt);
  if (state_->install_status == InstallStatus::Installed)
    throw std::logic_error("Ibex2 bindings are already installed");
  if (state_->install_status == InstallStatus::Spent)
    throw std::logic_error(
        "a previous Ibex2 binding installation failed; the Adapter is spent "
        "and the runtime must be discarded");

  // An Adapter has one installation attempt. Preflight failures have not
  // changed JavaScript, but consuming the attempt keeps retry behavior
  // deterministic. Once mutation starts, every failure below is additionally
  // reported as a terminal runtime failure.
  state_->install_status = InstallStatus::Spent;
  bool mutation_started = false;
  try {
    const void* endowed_state = ibex2_bindings_state(bindings);
    const void* grants = ibex2_bindings_grants(bindings);
    if (endowed_state == nullptr || grants == nullptr)
      throw std::invalid_argument("Ibex2 bindings require a live endowment");
    if (endowed_state != state_->queue)
      throw std::invalid_argument("Ibex2 bindings do not belong to this runtime state");
    validate_groups(groups);
    std::string fetch_primitives;
    std::string abort_hooks;
    std::string headers_hooks;
    std::unique_ptr<Reachability> reachability;
    auto read_bootstrap_name = [&](const char* option, const char* kind,
                                   Groups required, const char* group) {
      if (option == nullptr) return std::string{};
      std::string name(option);
      if (name.empty())
        throw std::invalid_argument(
            std::string(kind) + " require a non-empty global name");
      if (!is_ascii_identifier(name))
        throw std::invalid_argument(
            std::string(kind) + " global name must be an ASCII JavaScript "
            "identifier ([A-Za-z_$][A-Za-z0-9_$]*)");
      if (!has(groups, required))
        throw std::invalid_argument(
            std::string(kind) + " require the " + group + " group");
      // hasProperty follows the global's prototype chain, so inherited names
      // such as `toString` or `__proto__` collide as well.
      if (rt.global().hasProperty(rt, bootstrap_output_key(rt, name)))
        throw std::invalid_argument(
            std::string(kind) + " global already exists");
      return name;
    };
    fetch_primitives = read_bootstrap_name(
        options.fetch_primitives, "fetch primitives", GROUP_FETCH, "FETCH");
    abort_hooks = read_bootstrap_name(
        options.abort_hooks, "abort hooks", GROUP_ABORT, "ABORT");
    headers_hooks = read_bootstrap_name(
        options.headers_hooks, "headers hooks", GROUP_PURE, "PURE");
    const std::string* outputs[] = {&fetch_primitives, &abort_hooks,
                                    &headers_hooks};
    for (size_t i = 0; i < 3; ++i)
      for (size_t j = i + 1; j < 3; ++j)
        if (!outputs[i]->empty() && *outputs[i] == *outputs[j])
          throw std::invalid_argument(
              "trusted-bootstrap outputs require distinct global names");
    if (!fetch_primitives.empty() || !abort_hooks.empty() ||
        !headers_hooks.empty())
      reachability = std::make_unique<Reachability>(rt);
    auto expected = expected_scripts(groups);
    if (script_count != expected.size() || (script_count != 0 && scripts == nullptr))
      throw std::invalid_argument("Ibex2 binding bytecode count does not match groups");
    jsi::Value published_abort_hooks = jsi::Value::undefined();
    jsi::Value published_headers_hooks = jsi::Value::undefined();
    for (size_t i = 0; i < script_count; ++i) {
      if (scripts[i].name == nullptr || scripts[i].bytes == nullptr ||
          std::strcmp(scripts[i].name, expected[i]) != 0)
        throw std::invalid_argument("Ibex2 binding bytecode is not in scripts() order");
      validate_bytecode(scripts[i], state_->bytecode_version);
    }

    // Validation above is deliberately complete before the first host function
    // or JavaScript global is installed: one bad payload refuses the whole door.
    mutation_started = true;
    if (has(groups, GROUP_CONSOLE)) install_console(rt, state_->lifetime);
    if (has(groups, GROUP_PURE)) install_pure(rt, state_->lifetime);
    if (has(groups, GROUP_TIMERS)) install_timers(rt, state_->lifetime);
    if (has(groups, GROUP_CRYPTO)) install_crypto(rt, state_->lifetime);
    if (has(groups, GROUP_EVENTS)) install_events(rt, state_->lifetime);
    if (has(groups, GROUP_BLOB)) install_blob(rt, state_->lifetime);
    if (has(groups, GROUP_FETCH)) install_fetch(rt, *this, state_->lifetime);
#if defined(IBEX2_JSI_HAS_INTL)
    if (has(groups, GROUP_INTL)) {
      ibex2::intl_number_format::install(rt, state_->lifetime);
      ibex2::intl_case::install(rt, state_->lifetime);
    }
#endif

    for (size_t i = 0; i < script_count; ++i) {
      const auto& script = scripts[i];
      if (std::strcmp(script.name, "websocket") == 0) {
        rt.global().setProperty(rt, "__ibex2_fire_trusted_event",
                                jsi::Value(rt, state_->trusted_event_dispatch));
        if (state_->blob_helpers.isObject())
          rt.global().setProperty(rt, "__ibex2_blob_helpers",
                                  state_->blob_helpers);
      }
      if (std::strcmp(script.name, "fetch") == 0 &&
          state_->blob_helpers.isObject())
        rt.global().setProperty(rt, "__ibex2_blob_helpers",
                                state_->blob_helpers);
      auto buffer = std::make_shared<CompiledBytes>(script.bytes, script.len);
      auto value = rt.evaluateJavaScript(buffer, std::string(script.name) + ".js");
      if (std::strcmp(script.name, "timers") == 0) {
        if (!value.isObject() || !value.getObject(rt).isFunction(rt))
          throw jsi::JSError(
              rt, "timers binding did not return its private dispatcher");
        state_->timer_dispatch = jsi::Value(rt, value);
        continue;
      }
      if (std::strcmp(script.name, "abort") == 0 && !abort_hooks.empty()) {
        auto hooks = rt.global().getProperty(rt, "__ibex2_abort");
        if (!hooks.isObject())
          throw jsi::JSError(rt, "abort binding did not publish its hooks");
        auto object = hooks.getObject(rt);
        for (const char* member : {"own", "subscribe"}) {
          auto function = object.getProperty(rt, member);
          if (!function.isObject() || !function.getObject(rt).isFunction(rt))
            throw jsi::JSError(rt, "abort binding published invalid hooks");
        }
        freeze(rt, object);
        published_abort_hooks = jsi::Value(rt, object);
      }
      if (std::strcmp(script.name, "headers") == 0 && !headers_hooks.empty()) {
        auto hooks = rt.global().getProperty(rt, "__ibex2_headers_hooks");
        if (!hooks.isObject())
          throw jsi::JSError(rt, "headers binding did not publish its hooks");
        auto object = hooks.getObject(rt);
        auto guard = object.getProperty(rt, "guard");
        if (!guard.isObject() || !guard.getObject(rt).isFunction(rt))
          throw jsi::JSError(rt, "headers binding published invalid hooks");
        freeze(rt, object);
        published_headers_hooks = jsi::Value(rt, object);
      }
      if (std::strcmp(script.name, "blob") == 0) {
        if (!value.isObject())
          throw jsi::JSError(rt, "Blob binding did not evaluate to helpers");
        state_->blob_helpers = jsi::Value(rt, value);
        continue;
      }
      if (std::strcmp(script.name, "fetch") == 0) {
        if (!value.isObject() || !value.getObject(rt).isFunction(rt))
          throw jsi::JSError(rt, "fetch binding did not evaluate to a factory");
        state_->fetch_factory = jsi::Value(rt, value);
        continue;
      }
      if (std::strcmp(script.name, "websocket") == 0) {
        if (!value.isObject() || !value.getObject(rt).isFunction(rt))
          throw jsi::JSError(rt, "WebSocket binding did not evaluate to a factory");
        state_->websocket_factory = jsi::Value(rt, value);
        continue;
      }
      if (std::strcmp(script.name, "sqlite") == 0) {
        if (!value.isObject() || !value.getObject(rt).isFunction(rt))
          throw jsi::JSError(rt, "SQLite binding did not evaluate to a factory");
        state_->sqlite_factory = jsi::Value(rt, value);
        continue;
      }
      if (std::strcmp(script.name, "events") == 0) {
        if (!value.isObject())
          throw jsi::JSError(rt, "events binding did not return its engine hooks");
        auto hooks = value.getObject(rt);
        auto capture = [&](const char* name, jsi::Value& slot) {
          auto hook = hooks.getProperty(rt, name);
          if (!hook.isObject() || !hook.getObject(rt).isFunction(rt))
            throw jsi::JSError(rt, "events binding returned an invalid engine hook");
          slot = jsi::Value(rt, hook);
        };
        capture("reportException", state_->event_reporter);
        capture("onUnhandled", state_->rejection_unhandled);
        capture("onHandled", state_->rejection_handled);
        capture("fireTrustedEvent", state_->trusted_event_dispatch);
        capture("hasEventListener", state_->event_listener_query);
        capture("setListenerChangeHook", state_->event_listener_change_hook);
        if (has(groups, GROUP_ABORT)) {
          // The hook crosses only the next installation step. abort.js takes
          // and deletes it before any application entrance can run.
          rt.global().setProperty(
              rt, "__ibex2_fire_trusted_event",
              jsi::Value(rt, state_->trusted_event_dispatch));
          auto set_abort_hooks = hooks.getProperty(rt, "setAbortHooks");
          if (!set_abort_hooks.isObject() ||
              !set_abort_hooks.getObject(rt).isFunction(rt))
            throw jsi::JSError(
                rt, "events binding returned an invalid abort-hook bridge");
          rt.global().setProperty(
              rt, "__ibex2_set_event_abort_hooks", std::move(set_abort_hooks));
        }
        continue;
      }
#if defined(IBEX2_JSI_HAS_INTL)
      if (std::strcmp(script.name, "intl_datetime") == 0) {
        if (!value.isObject() || !value.getObject(rt).isFunction(rt))
          throw jsi::JSError(rt, "DateTimeFormat binding did not evaluate to a factory");
        auto arguments =
            ibex2::intl_datetime::factory_arguments(rt, state_->lifetime);
        value.getObject(rt).getFunction(rt).call(
            rt, static_cast<const jsi::Value*>(arguments.data()), arguments.size());
      }
#endif
    }

    if (has(groups, GROUP_EVENTS) && has(groups, GROUP_ABORT)) {
      remove_global(rt, rt.global(), "__ibex2_fire_trusted_event");
      remove_global(rt, rt.global(), "__ibex2_set_event_abort_hooks");
    }
    if (has(groups, GROUP_WEBSOCKET)) {
      remove_global(rt, rt.global(), "__ibex2_fire_trusted_event");
    }

    // @ref LLP 0057.000#l3--events-abort-and-the-second-direction — vanilla Hermes has delayed tracker callbacks, not the patched checkpoint hook
    // Stock Hermes exposes its JavaScript Promise rejection tracker through
    // HermesInternal. The upstream tracker uses setTimeout (100 ms for the
    // standard programmer-error classes, two seconds otherwise), so EVENTS
    // remains independent and tracking is enabled only when TIMERS was also
    // selected. Engines without this optional intrinsic simply omit the two
    // rejection events; ordinary EventTarget and reportError still work.
    if (has(groups, GROUP_EVENTS) && has(groups, GROUP_TIMERS)) {
      auto internal_value = rt.global().getProperty(rt, "HermesInternal");
      if (internal_value.isObject()) {
        auto internal = internal_value.getObject(rt);
        auto enable = internal.getProperty(rt, "enablePromiseRejectionTracker");
        if (enable.isObject() && enable.getObject(rt).isFunction(rt) &&
            state_->rejection_unhandled.isObject() &&
            state_->rejection_handled.isObject()) {
          jsi::Object options(rt);
          options.setProperty(rt, "allRejections", true);
          options.setProperty(rt, "onUnhandled",
                              jsi::Value(rt, state_->rejection_unhandled));
          options.setProperty(rt, "onHandled",
                              jsi::Value(rt, state_->rejection_handled));
          enable.getObject(rt).getFunction(rt).callWithThis(rt, internal, options);
          // The upstream tracker installs its callbacks in Hermes's private
          // Promise._B/_C slots. They are trusted engine mutations made before
          // hardening, so advance the integrity baseline to those exact
          // function identities; the ordinary harden walk freezes them.
          accept_trusted_intrinsic_property(
              rt.global().getPropertyAsObject(rt, "Promise"), "_B");
          accept_trusted_intrinsic_property(
              rt.global().getPropertyAsObject(rt, "Promise"), "_C");
        }
      }
    }

#if defined(IBEX2_JSI_HAS_INTL)
    if (has(groups, GROUP_INTL)) {
      auto global = rt.global();
      auto accept = [&](const char* constructor, const char* property) {
        auto prototype = global.getPropertyAsObject(rt, constructor)
                             .getPropertyAsObject(rt, "prototype");
        accept_trusted_intrinsic_property(std::move(prototype), property);
      };
      accept("Number", "toLocaleString");
      accept("BigInt", "toLocaleString");
      accept("String", "toLocaleLowerCase");
      accept("String", "toLocaleUpperCase");
    }
#endif

    state_->groups = groups;
    auto global = rt.global();
    if (has(groups, GROUP_FETCH)) {
      global.setProperty(rt, "fetch", fetch(grants));
    } else if (has(groups, GROUP_PURE)) {
      for (const char* name : {"__ibex2_headers_free", "__ibex2_headers_hooks",
                               "__ibex2_text_encode", "__ibex2_text_decode",
                               "__ibex2_text_encode_into"})
        remove_global(rt, global, name);
    }
    if (has(groups, GROUP_WEBSOCKET))
      global.setProperty(rt, "WebSocket", websocket(grants));
    if (has(groups, GROUP_ABORT) && !has(groups, GROUP_FETCH))
      remove_global(rt, global, "__ibex2_abort");
    if (has(groups, GROUP_STORAGE)) {
      auto storage_value = storage(grants);
      global.setProperty(rt, "fs", storage_value.getProperty(rt, "fs"));
      global.setProperty(rt, "sqlite", storage_value.getProperty(rt, "sqlite"));
    }
    if (has(groups, GROUP_ENV))
      global.setProperty(rt, "process", make_process(rt, grants));
    auto publish_bootstrap_output = [&](const char* kind,
                                        const std::string& name,
                                        jsi::Object object,
                                        const auto& members) {
      if (global.hasProperty(rt, bootstrap_output_key(rt, name)))
        throw std::invalid_argument(
            std::string(kind) + " global collides with an installed binding");
      State::BootstrapOutput output{kind, name, {}};
      output.identities.emplace_back(
          std::string("the ") + kind + " object", jsi::Value(rt, object));
      for (const char* member : members)
        output.identities.emplace_back(
            std::string(kind) + " member " + member,
            object.getProperty(rt, member));
      global.setProperty(rt, bootstrap_output_key(rt, name),
                         jsi::Value(rt, object));
      state_->bootstrap_outputs.push_back(std::move(output));
    };
    if (!fetch_primitives.empty()) {
      auto primitives =
          make_fetch_primitives(rt, *this, state_->lifetime, grants);
      publish_bootstrap_output(
          "fetch primitives", fetch_primitives, std::move(primitives),
          kFetchPrimitiveMembers);
    }
    if (!abort_hooks.empty()) {
      if (!published_abort_hooks.isObject())
        throw jsi::JSError(rt, "abort binding hooks were not retained");
      constexpr const char* members[] = {"own", "subscribe"};
      publish_bootstrap_output(
          "abort hooks", abort_hooks,
          published_abort_hooks.getObject(rt), members);
    }
    if (!headers_hooks.empty()) {
      if (!published_headers_hooks.isObject())
        throw jsi::JSError(rt, "headers binding hooks were not retained");
      constexpr const char* members[] = {"guard"};
      publish_bootstrap_output(
          "headers hooks", headers_hooks,
          published_headers_hooks.getObject(rt), members);
    }
    if (!state_->bootstrap_outputs.empty())
      state_->reachability = std::move(reachability);

    if (options.defer_intrinsic_snapshot) {
      // Installation itself may make narrow trusted intrinsic replacements.
      // Discard the constructor-time baseline only after all installation
      // mutations succeed; capture_intrinsics() will record the embedder's
      // complete post-prelude baseline before hardening. The API snapshots the
      // realm unconditionally; the embedder is responsible for running only
      // its trusted prelude in the interval.
      state_->integrity.reset();
      state_->intrinsic_snapshot_deferred = true;
    }

    state_->install_status = InstallStatus::Installed;
  } catch (const jsi::JSIException& error) {
    if (mutation_started)
      throw std::runtime_error(
          std::string("Ibex2 binding installation failed after mutating the runtime; ") +
          "the runtime must be discarded: " + error.what());
    throw;
  }
  // Native engine exceptions retain their type; the caller discards the runtime.
}

void Adapter::verify_fetch_primitives_unreachable() {
  verify_trusted_bootstrap_unreachable();
}

void Adapter::verify_trusted_bootstrap_unreachable() {
  if (!runtime_) throw std::logic_error("Ibex2 bindings are detached");
  auto& rt = *runtime_;
  if (state_->bootstrap_outputs.empty()) return;
  for (const auto& output : state_->bootstrap_outputs) {
    if (rt.global().hasProperty(rt, bootstrap_output_key(rt, output.name)))
      throw std::runtime_error(
          "refusing to harden while " + output.kind + " global \"" +
          output.name + "\" is present");
  }
  for (const auto& output : state_->bootstrap_outputs)
    require_unreachable(rt, *state_->reachability, output.identities);
}

void Adapter::verify_harden_preconditions() {
  if (!runtime_) throw std::logic_error("Ibex2 bindings are detached");
  if (!state_->integrity)
    throw std::runtime_error(
        "refusing to harden before the deferred intrinsic snapshot is captured");
  verify_trusted_bootstrap_unreachable();
  state_->integrity->require(*runtime_, false);
}

void Adapter::harden(const CompiledScript& script) {
  if (!runtime_) throw std::logic_error("Ibex2 bindings are detached");
  auto& rt = *runtime_;
  state_->require(rt);
  if (script.bytes == nullptr)
    throw std::invalid_argument("Ibex2 harden requires its compiled bytecode");
  validate_bytecode(script, state_->bytecode_version);
  verify_harden_preconditions();
  // The guard and harden.js each allocate mostly short-lived descriptor and
  // traversal objects. Collect between them so harden.js reuses the young
  // generation pages after the guard's rooted identities have been checked.
  if (!state_->bootstrap_outputs.empty())
    rt.instrumentation().collectGarbage("ibex2 harden guard");
  state_->hardened = true;
  rt.evaluateJavaScript(std::make_shared<CompiledBytes>(script.bytes, script.len),
                        "harden.js");
  // The trusted script replaced writable prototype data with accessors.
  // Its preconditions verified the old identities; pin the frozen result.
  state_->integrity = std::make_unique<Integrity>(rt);
  state_->integrity->require(rt);
}

static jsi::Value filesystem_promise(jsi::Runtime& r, jsi::Value value, uint32_t op) {
  auto promise = value.getObject(r);
  auto convert = jsi::Function::createFromHostFunction(r,
      jsi::PropNameID::forAscii(r, "filesystemResult"), 1,
      [op](jsi::Runtime& r, const jsi::Value&, const jsi::Value* args, size_t count) {
        if (count != 1 || !args[0].isString())
          throw jsi::JSError(r, "invalid filesystem result");
        auto text = args[0].getString(r).utf8(r);
        if (op == 113) {
          std::vector<std::string> names;
          for (size_t start = 0; start < text.size();) {
            auto end = text.find('\0', start);
            if (end == std::string::npos) end = text.size();
            names.push_back(text.substr(start, end - start));
            start = end + 1;
          }
          jsi::Array result(r, names.size());
          for (size_t i = 0; i < names.size(); ++i)
            result.setValueAtIndex(r, i, jsi::String::createFromUtf8(r, names[i]));
          return jsi::Value(r, result);
        }
        std::vector<std::string> fields;
        size_t start = 0;
        for (;;) {
          auto end = text.find('\t', start);
          fields.push_back(text.substr(start, end == std::string::npos ? end : end - start));
          if (end == std::string::npos) break;
          start = end + 1;
        }
        if (fields.size() != 4 && fields.size() != 6) throw jsi::JSError(r, "invalid filesystem stat");
        jsi::Object result(r);
        result.setProperty(r, "size", std::stod(fields[0]));
        result.setProperty(r, "isFile", fields[1] == "1");
        result.setProperty(r, "isDirectory", fields[2] == "1");
        result.setProperty(r, "modifiedMs", std::stod(fields[3]));
        // Exact patch 24: the permission bits, and whether an lstat met a link.
        if (fields.size() == 6) {
          result.setProperty(r, "mode", std::stod(fields[4]));
          result.setProperty(r, "isSymbolicLink", fields[5] == "1");
        }
        return jsi::Value(r, result);
      });
  return promise.getPropertyAsFunction(r, "then").callWithThis(r, promise, convert);
}

jsi::Function Adapter::async_binding(const char* name, uint32_t op, const void* grants) {
  if (!runtime_) throw std::logic_error("Ibex2 bindings are detached");
  auto& rt = *runtime_;
  state_->require(rt);
  auto state = state_;
  std::shared_ptr<const void> authority(ibex2_grants_retain(grants), ibex2_grants_destroy);
  return jsi::Function::createFromHostFunction(rt, jsi::PropNameID::forAscii(rt, name), 1,
      [state, authority, op](jsi::Runtime& r, const jsi::Value&, const jsi::Value* args, size_t count) {
        state->require(r);
        if (op == 150) {
          if (!state->integrity)
            throw jsi::JSError(
                r, "Ibex2 SQLite requires capture_intrinsics() before use");
          state->integrity->require(r);
        }
        std::vector<std::string> owned;
        std::vector<Ibex2AbiValue> abi;
        owned.reserve(count); abi.reserve(count);
        for (size_t i = 0; i < count; ++i) abi.push_back(to_abi(r, args[i], owned));
        uint64_t id = state->next_task_id++;
        auto executor = jsi::Function::createFromHostFunction(r,
            jsi::PropNameID::forAscii(r, "executor"), 2,
            [state, id](jsi::Runtime& r, const jsi::Value&, const jsi::Value* args, size_t count) {
              state->require(r);
              if (count < 2) throw jsi::JSError(r, "promise executor needs resolve and reject");
              state->pending.emplace(id, State::Pending{
                  args[0].getObject(r).getFunction(r), args[1].getObject(r).getFunction(r)});
              return jsi::Value::undefined();
            });
        jsi::Value promise;
        try {
          auto ctor = r.global().getPropertyAsFunction(r, "Promise");
          promise = ctor.callAsConstructor(r, executor);
          if (op == 121) {
            set_native_state_once(r, promise.getObject(r),
                std::make_shared<State::ImagePromiseOwner>(id, state),
                "Image operation Promise");
          }
          if (op == 113 || op == 116 || op == 122)
            promise = filesystem_promise(r, std::move(promise), op);
        } catch (...) {
          // A constructor can invoke the executor and then throw. Finish every
          // fallible Promise/brand step before accepting a worker, and release
          // any callbacks it registered when preparation fails.
          state->pending.erase(id);
          throw;
        }
        if (ibex2_async_begin(state->queue, authority.get(), op, abi.data(), abi.size(), id) != 0) {
          state->pending.erase(id);
          throw jsi::JSError(r, "could not start the async operation");
        }
        return promise;
      });
}

uint64_t Adapter::image_task_id(jsi::Runtime& rt, const jsi::Value& promise) const {
  // Reject foreign runtimes before inspecting a runtime-owned JSI value.
  if (!runtime_ || runtime_ != &rt || !state_->alive || !promise.isObject()) return 0;
  auto object = promise.getObject(rt);
  if (!object.hasNativeState<State::ImagePromiseOwner>(rt)) return 0;
  auto brand = object.getNativeState<State::ImagePromiseOwner>(rt);
  auto owner = brand->owner.lock();
  if (owner != state_ || state_->pending.find(brand->task_id) == state_->pending.end()) return 0;
  return brand->task_id;
}

jsi::Function Adapter::fetch(const void* grants) {
  if (!runtime_) throw std::logic_error("Ibex2 bindings are detached");
  auto& rt = *runtime_;
  state_->require(rt);
  if (!has(state_->groups, GROUP_FETCH) || !state_->fetch_factory.isObject() ||
      !state_->fetch_factory.getObject(rt).isFunction(rt))
    throw std::logic_error("Ibex2 FETCH group is not installed");
  auto raw = async_binding("fetch", 101, grants);
  return state_->fetch_factory.getObject(rt).getFunction(rt)
      .call(rt, raw).getObject(rt).getFunction(rt);
}

jsi::Function Adapter::websocket(const void* grants) {
  if (!runtime_) throw std::logic_error("Ibex2 bindings are detached");
  auto& rt = *runtime_;
  state_->require(rt);
  if (!has(state_->groups, GROUP_WEBSOCKET) ||
      !state_->websocket_factory.isObject() ||
      !state_->websocket_factory.getObject(rt).isFunction(rt) ||
      !state_->event_listener_change_hook.isObject() ||
      !state_->event_listener_change_hook.getObject(rt).isFunction(rt))
    throw std::logic_error("Ibex2 WEBSOCKET group is not installed");
  return state_->websocket_factory.getObject(rt).getFunction(rt)
      .call(rt, websocket_hooks(grants), state_->event_listener_change_hook)
      .getObject(rt).getFunction(rt);
}

jsi::Object Adapter::storage(const void* grants) {
  if (!runtime_) throw std::logic_error("Ibex2 bindings are detached");
  auto& rt = *runtime_;
  state_->require(rt);
  if (!has(state_->groups, GROUP_STORAGE) || !state_->sqlite_factory.isObject() ||
      !state_->sqlite_factory.getObject(rt).isFunction(rt))
    throw std::logic_error("Ibex2 STORAGE group is not installed");
  return storage(grants, state_->sqlite_factory.getObject(rt).getFunction(rt));
}

namespace {
struct SqliteOwner final : jsi::NativeState {
  void* owner;
  explicit SqliteOwner(void* value) : owner(value) {}
  ~SqliteOwner() override { ibex2_sqlite_owner_destroy(owner); }
};
void freeze(jsi::Runtime& rt, const jsi::Object& value) {
  rt.global().getPropertyAsObject(rt, "Object").getPropertyAsFunction(rt, "freeze").call(rt, value);
}
}

jsi::Object Adapter::storage(const void* grants, const jsi::Function& factory) {
  if (!runtime_) throw std::logic_error("Ibex2 bindings are detached");
  auto& rt = *runtime_;
  state_->require(rt);
  struct Method { const char* name; uint32_t op; };
  static const Method fs_methods[] = {
      {"readFile",110}, {"writeFile",111}, {"appendFile",112}, {"readdir",113},
      {"mkdir",114}, {"rm",115}, {"stat",116}, {"rename",117}, {"copyFile",118},
      {"realpath",119}, {"atomicWriteFile",120}, {"compressImage",121},
      {"transformImage",121},
      // Exact patch 24 (exact2 LLP 1133.007 D6a).
      {"lstat",122}, {"readlink",123}, {"access",124}, {"chmod",125},
      {"symlink",126}, {"digest",127}, {"flock",128}, {"unlock",129}};
  jsi::Object fs(rt);
  for (const auto& method : fs_methods)
    fs.setProperty(rt, method.name, async_binding(method.name, method.op, grants));
  jsi::Object directories(rt);
  directories.setProperty(rt, "data", "app:/data");
  directories.setProperty(rt, "cache", "app:/cache");
  directories.setProperty(rt, "temporary", "app:/tmp");
  freeze(rt, directories);
  fs.setProperty(rt, "directories", directories);
  freeze(rt, fs);

  auto state = state_;
  auto field = jsi::Function::createFromHostFunction(rt,
      jsi::PropNameID::forAscii(rt, "sqliteResult"), 1,
      [state](jsi::Runtime& r, const jsi::Value&, const jsi::Value* args, size_t count) {
        state->require(r);
        return call_host(r, state->queue, 80, args, count);
      });
  auto retain = jsi::Function::createFromHostFunction(rt,
      jsi::PropNameID::forAscii(rt, "sqliteOwn"), 3,
      [state](jsi::Runtime& r, const jsi::Value&, const jsi::Value* args, size_t count) {
        state->require(r);
        if (count != 3 || !args[0].isNumber() || !args[2].isObject())
          throw jsi::JSError(r, "SQLite owner needs a handle, kind, and object");
        const int kind = static_cast<int>(primitive_integer(
            r, args[1], 0.0, 1.0, "SQLite owner kind"));
        set_native_state_once(r, args[2].getObject(r),
            std::make_shared<SqliteOwner>(ibex2_sqlite_owner_create(
                state->queue, args[0].asNumber(),
                kind)),
            "SQLite owner");
        return jsi::Value::undefined();
      });
  auto make_sqlite = factory.call(rt, field, retain).getObject(rt).getFunction(rt);
  static const Method sqlite_methods[] = {
      {"open",150}, {"prepare",151}, {"execute",152}, {"query",153},
      {"statementExecute",154}, {"statementQuery",155}, {"transaction",156},
      {"close",157}, {"statementClose",158}};
  jsi::Object raw(rt);
  for (const auto& method : sqlite_methods)
    raw.setProperty(rt, method.name, async_binding(method.name, method.op, grants));
  jsi::Object result(rt);
  result.setProperty(rt, "fs", fs);
  result.setProperty(rt, "sqlite", make_sqlite.call(rt, raw));
  freeze(rt, result);
  return result;
}

void Adapter::settle(uint64_t id, Ibex2AbiValue& value, bool is_error) {
  struct Release { Ibex2AbiValue& value; ~Release() { ibex2_host_release(&value); } } release{value};
  auto found = state_->pending.find(id);
  if (!state_->alive || found == state_->pending.end()) return;
  auto promise = std::move(found->second);
  state_->pending.erase(found);
  auto& rt = *runtime_;
  auto payload = from_abi(rt, value);
  if (is_error) {
    // The realm's Error -- its lookup and its construction, each JavaScript
    // the embedder may have replaced -- may detach the adapter: no further
    // JavaScript once it has (LLP 0071 D5).
    auto make = rt.global().getPropertyAsFunction(rt, "Error");
    if (!state_->alive || !runtime_) return;
    auto error = make.callAsConstructor(rt, payload);
    if (!state_->alive || !runtime_) return;
    promise.reject.call(rt, error);
  } else promise.resolve.call(rt, payload);
}

void Adapter::fire_timer(uint64_t id) {
  if (!runtime_) throw std::logic_error("Ibex2 bindings are detached");
  auto& rt = *runtime_;
  state_->require(rt);
  if (!has(state_->groups, GROUP_TIMERS) ||
      !state_->timer_dispatch.isObject() ||
      !state_->timer_dispatch.getObject(rt).isFunction(rt))
    throw std::logic_error("Ibex2 TIMERS group is not installed");
  state_->timer_dispatch.getObject(rt).getFunction(rt).call(
      rt, static_cast<double>(id));
}

#if defined(IBEX2_TEST_SUPPORT)
void Adapter::assert_timer_dispatch_unreachable_for_test() const {
  if (!runtime_) throw std::logic_error("Ibex2 bindings are detached");
  auto& rt = *runtime_;
  if (!state_->timer_dispatch.isObject() ||
      !state_->timer_dispatch.getObject(rt).isFunction(rt))
    throw std::logic_error("Ibex2 TIMERS group is not installed");
  Reachability walk(rt);
  std::vector<std::pair<std::string, jsi::Value>> identities;
  identities.emplace_back(
      "the private timer dispatcher", jsi::Value(rt, state_->timer_dispatch));
  require_unreachable(rt, walk, identities, 4096, 65536);
}
#endif

uint64_t Adapter::subscribe(jsi::Function callback) {
  if (!runtime_) throw std::logic_error("Ibex2 bindings are detached");
  state_->require(*runtime_);
  uint64_t id = 0;
  void* rust = ibex2_subscription_create(state_->queue, &id);
  if (rust == nullptr)
    throw std::runtime_error("could not create Ibex2 event subscription");
  try {
    state_->subscriptions.emplace(
        id, State::EventSubscription{
                rust, std::move(callback), 0, nullptr,
                jsi::Value::undefined(), false, {}});
  } catch (...) {
    ibex2_subscription_destroy(rust);
    throw;
  }
  return id;
}

void Adapter::unsubscribe(uint64_t id) {
  auto found = state_->subscriptions.find(id);
  if (found == state_->subscriptions.end()) return;
  // Rust cancellation is the barrier for the host-task FIFO; only after it
  // returns is the owner-thread JSI root released.
  if (auto owner = found->second.native_owner.lock())
    owner->unsubscribe_events();
  else
    ibex2_subscription_destroy(found->second.rust);
  state_->subscriptions.erase(found);
}

void Adapter::refresh_websocket_keepalives() {
  if (!runtime_ || !state_->alive) return;
  auto& rt = *runtime_;
  for (auto found = state_->subscriptions.begin();
       found != state_->subscriptions.end();) {
    auto& subscription = found->second;
    if (subscription.websocket == 0 || subscription.weak_owner == nullptr) {
      ++found;
      continue;
    }
    auto native = subscription.native_owner.lock();
    auto owner = subscription.weak_owner->lock(rt);
    if (!native || !owner.isObject()) {
      found = state_->subscriptions.erase(found);
      continue;
    }
    auto has_listener = [&](const char* type) {
      if (!state_->event_listener_query.isObject() ||
          !state_->event_listener_query.getObject(rt).isFunction(rt))
        return false;
      auto result = state_->event_listener_query.getObject(rt)
          .getFunction(rt).call(
              rt, owner, jsi::String::createFromAscii(rt, type));
      return result.isBool() && result.getBool();
    };
    int ready = ibex2_websocket_ready_state(state_->queue,
                                            subscription.websocket);
    subscription.listener_keepalive = ready == 0
        ? has_listener("open") || has_listener("message") ||
              has_listener("error") || has_listener("close")
        : ready == 1
        ? has_listener("message") || has_listener("error") ||
              has_listener("close")
        : has_listener("error") || has_listener("close");
    bool keep = subscription.listener_keepalive ||
        (ready == 1 &&
         ibex2_websocket_buffered_amount(state_->queue,
                                         subscription.websocket) != 0);
    subscription.strong_owner = keep
        ? jsi::Value(rt, owner) : jsi::Value::undefined();
    ++found;
  }
}

void Adapter::prepare_garbage_collection() {
  refresh_websocket_keepalives();
}

size_t Adapter::websocket_keepalive_count_for_test() const {
  size_t count = 0;
  for (const auto& entry : state_->subscriptions) {
    if (entry.second.websocket != 0 && entry.second.strong_owner.isObject())
      ++count;
  }
  return count;
}

void Adapter::deliver_event(uint64_t id, Ibex2AbiValue& value) {
  struct Release {
    Ibex2AbiValue& value;
    ~Release() { ibex2_host_release(&value); }
  } release{value};
  auto found = state_->subscriptions.find(id);
  if (!state_->alive || found == state_->subscriptions.end()) return;
  bool terminal = found->second.websocket != 0 &&
      value.tag == IBEX2_TAG_BYTES && value.data != nullptr &&
      value.len > 0 && value.data[0] == 4;
  auto payload = from_abi(*runtime_, value);
  jsi::Value owner = jsi::Value::undefined();
  if (found->second.websocket != 0) {
    if (found->second.strong_owner.isObject())
      owner = jsi::Value(*runtime_, found->second.strong_owner);
    else if (found->second.weak_owner != nullptr)
      owner = found->second.weak_owner->lock(*runtime_);
    if (!owner.isObject()) {
      unsubscribe(id);
      return;
    }
  }
  try {
    if (found->second.websocket != 0)
      found->second.callback.call(*runtime_, owner, payload);
    else
      found->second.callback.call(*runtime_, payload);
  } catch (...) {
    if (terminal) unsubscribe(id);
    throw;
  }
  if (terminal) unsubscribe(id);
  else refresh_websocket_keepalives();
}

void Adapter::report_error(const jsi::Value& error) {
  if (!runtime_) throw std::logic_error("Ibex2 bindings are detached");
  auto& rt = *runtime_;
  if (state_->event_reporter.isObject() &&
      state_->event_reporter.getObject(rt).isFunction(rt)) {
    state_->event_reporter.getObject(rt).getFunction(rt).call(rt, error);
    return;
  }
  auto message = error.toString(rt).utf8(rt);
  ibex2_report_uncaught(message.c_str());
}

void Adapter::report_error(const char* message) {
  if (!runtime_) throw std::logic_error("Ibex2 bindings are detached");
  auto& rt = *runtime_;
  if (state_->event_reporter.isObject() &&
      state_->event_reporter.getObject(rt).isFunction(rt)) {
    // As in settle(): Error's lookup and its construction may each detach
    // the adapter.
    auto detached = [&] { return !state_->alive || !runtime_ || !state_->event_reporter.isObject(); };
    auto make = rt.global().getPropertyAsFunction(rt, "Error");
    if (detached()) {
      ibex2_report_uncaught(message);
      return;
    }
    auto error = make.callAsConstructor(rt, message == nullptr ? "uncaught error" : message);
    if (detached()) {
      ibex2_report_uncaught(message);
      return;
    }
    state_->event_reporter.getObject(rt).getFunction(rt).call(rt, error);
    return;
  }
  ibex2_report_uncaught(message);
}

namespace {
// Holds the drive flag and a reference to the runtime state, releasing both
// on every way out, a throw included (LLP 0071 D5).
struct DriveHold {
  const void* held;
  explicit DriveHold(const void* queue) : held(ibex2_adapter_begin_drive(queue)) {}
  ~DriveHold() { if (held) ibex2_adapter_end_drive(held); }
  DriveHold(const DriveHold&) = delete;
  DriveHold& operator=(const DriveHold&) = delete;
  bool taken() const { return held != nullptr; }
};
}  // namespace

bool Adapter::deliver_one() {
  if (!state_->alive) return false;
  // @ref LLP 0071#d5--the-cycle-is-the-adapters-and-does-not-nest — one host task at a time: a delivery reached from inside another, a cycle, or an owning pump is refused before it takes anything
  DriveHold drive(state_->queue);
  if (!drive.taken())
    throw std::logic_error(
        "Ibex2 deliver_one: nested delivery (a delivery, a cycle or an owning "
        "pump is already running on this runtime)");
  return deliver_next();
}

// A microtask checkpoint that reports what it cannot finish, as the owning
// pump's does (LLP 0058.000.000 §8): a job that throws out of the queue -- on
// the ordinary paths only an engine-raised error can, though a raw
// HermesInternal.enqueueJob job can too -- is reported and the drain resumes behind it,
// the engine having retired the job before running it. Returns false when the
// adapter was detached by the JavaScript it ran.
bool Adapter::checkpoint() {
  for (;;) {
    if (!state_->alive || !runtime_) return false;
    try {
      runtime_->drainMicrotasks();
      return state_->alive && runtime_;
    } catch (const jsi::JSError& error) {
      if (!state_->alive || !runtime_) return false;
      try {
        report_error(error.value());
      } catch (const jsi::JSIException&) {
        ibex2_report_uncaught(error.getMessage().c_str());
      }
    } catch (const jsi::JSIException& error) {
      if (!state_->alive || !runtime_) return false;
      try {
        report_error(error.what());
      } catch (const jsi::JSIException&) {
        ibex2_report_uncaught(error.what());
      }
    }
  }
}

Adapter::Cycle Adapter::cycle() {
  if (!state_->alive) return Cycle::Idle;
  // @ref LLP 0058.000.000#8-tasks-microtasks-timers-and-callbacks — a drive requested while one runs records nothing and returns, as the owning pump does
  DriveHold drive(state_->queue);
  if (!drive.taken()) return Cycle::Nested;
  const void* queue = state_->queue;
  if (!checkpoint()) return Cycle::Idle;
  ibex2_adapter_admit_due_timers(queue);
  bool delivered = deliver_next();
  // A callback that detached the adapter leaves no runtime to checkpoint.
  checkpoint();
  return delivered ? Cycle::Delivered : Cycle::Idle;
}

bool Adapter::deliver_next() {
  int kind = 0, is_error = 0;
  unsigned long long id = 0;
  Ibex2AbiValue value{IBEX2_TAG_UNDEFINED, 0, nullptr, 0};
  if (!ibex2_take_task(state_->queue, &kind, &id, &value, &is_error)) return false;
  if (kind != 1 && kind != 2 && kind != 3) {
    ibex2_host_release(&value);
    throw jsi::JSError(*runtime_, "Ibex2 adapter received an unknown task kind");
  }
  // @ref LLP 0058.000.000#8-tasks-microtasks-timers-and-callbacks — borrowed-adapter task failures are reported like owning-pump failures
  // @ref LLP 0071#d1--a-borrowed-adapter-delivers-timer-tasks — a due timer is delivered like an event, inside the same containment
  try {
    if (kind == 3)
      deliver_event(id, value);
    else if (kind == 2)
      fire_timer(id);
    else
      settle(id, value, is_error != 0);
  } catch (const jsi::JSError& error) {
    try {
      report_error(error.value());
    } catch (const jsi::JSIException&) {
      std::string message = error.getMessage();
      const std::string& stack = error.getStack();
      if (!stack.empty()) {
        message += "\n";
        message += stack;
      }
      ibex2_report_uncaught(message.c_str());
    }
  } catch (const jsi::JSIException& error) {
    try {
      report_error(error.what());
    } catch (const jsi::JSIException&) {
      ibex2_report_uncaught(error.what());
    }
  }
  return true;
}
} // namespace ibex2::jsi_adapter
