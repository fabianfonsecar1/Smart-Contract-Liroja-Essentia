//! Contrato de pedidos de vino para hoteles - Liroja Essentia
//!
//! El hotel paga su pedido y el dinero queda guardado en el contrato.
//! Liroja Essentia recibe el pago solo cuando el hotel confirma que recibió
//! las botellas. Si Liroja todavía no ha enviado el pedido, el hotel puede
//! cancelarlo y recuperar su dinero.
//!
//! Usa las librerías de OpenZeppelin para Stellar:
//! - `Ownable`  (stellar-access): el dueño es Liroja Essentia.
//! - `Pausable` (stellar-contract-utils): botón de emergencia para detener pedidos nuevos.

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, panic_with_error, token,
    Address, Env, Symbol,
};
use stellar_access::ownable::{self as ownable, Ownable};
use stellar_contract_utils::pausable::{self as pausable, Pausable};
use stellar_macros::{only_owner, when_not_paused};

/// Días que tiene el hotel para confirmar la entrega. Si no confirma ni
/// reclama en ese tiempo, Liroja Essentia puede cobrar el pedido.
pub const DIAS_PARA_CONFIRMAR: u64 = 7;
const SEGUNDOS_POR_DIA: u64 = 24 * 60 * 60;

// ---------- Datos ----------

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Estado {
    /// El hotel pagó; el dinero está guardado en el contrato.
    Pagado,
    /// Liroja Essentia envió las botellas.
    Enviado,
    /// El hotel confirmó que recibió el pedido; Liroja ya cobró.
    Entregado,
    /// El hotel canceló antes del envío y recuperó su dinero.
    Cancelado,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pedido {
    pub hotel: Address,
    pub vino: Symbol,
    pub botellas: u32,
    pub total: i128,
    pub estado: Estado,
    /// Momento (segundos) en que se marcó como enviado. 0 si no se ha enviado.
    pub fecha_envio: u64,
}

#[contracttype]
pub enum DataKey {
    Token,
    SiguienteId,
    Precio(Symbol),
    Pedido(u32),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    VinoNoExiste = 1,
    CantidadInvalida = 2,
    PrecioInvalido = 3,
    PedidoNoExiste = 4,
    EstadoIncorrecto = 5,
    NoEsElHotel = 6,
    PlazoNoVencido = 7,
}

// ---------- Eventos ----------

#[contractevent]
pub struct PedidoCreado {
    #[topic]
    pub id: u32,
    pub hotel: Address,
    pub total: i128,
}

#[contractevent]
pub struct PedidoActualizado {
    #[topic]
    pub id: u32,
    pub estado: Estado,
}

// ---------- Contrato ----------

#[contract]
pub struct PedidosHotel;

#[contractimpl]
impl PedidosHotel {
    /// `dueno`: cuenta de Liroja Essentia. `token`: moneda de pago (ej. USDC o XLM).
    pub fn __constructor(e: &Env, dueno: Address, token: Address) {
        ownable::set_owner(e, &dueno);
        e.storage().instance().set(&DataKey::Token, &token);
        e.storage().instance().set(&DataKey::SiguienteId, &1u32);
    }

    // ----- Funciones de Liroja Essentia (dueño) -----

    /// Agrega o cambia el precio por botella de un vino del catálogo.
    #[only_owner]
    pub fn poner_precio(e: &Env, vino: Symbol, precio: i128) {
        if precio <= 0 {
            panic_with_error!(e, Error::PrecioInvalido);
        }
        e.storage().persistent().set(&DataKey::Precio(vino), &precio);
    }

    /// Liroja marca que ya envió las botellas al hotel.
    #[only_owner]
    pub fn marcar_enviado(e: &Env, id: u32) {
        let mut pedido = leer_pedido(e, id);
        if pedido.estado != Estado::Pagado {
            panic_with_error!(e, Error::EstadoIncorrecto);
        }
        pedido.estado = Estado::Enviado;
        pedido.fecha_envio = e.ledger().timestamp();
        guardar(e, id, &pedido);
    }

    /// Si el hotel no confirma en `DIAS_PARA_CONFIRMAR` días después del envío,
    /// Liroja puede cobrar el pedido.
    #[only_owner]
    pub fn cobrar_sin_confirmacion(e: &Env, id: u32) {
        let mut pedido = leer_pedido(e, id);
        if pedido.estado != Estado::Enviado {
            panic_with_error!(e, Error::EstadoIncorrecto);
        }
        let limite = pedido.fecha_envio + DIAS_PARA_CONFIRMAR * SEGUNDOS_POR_DIA;
        if e.ledger().timestamp() < limite {
            panic_with_error!(e, Error::PlazoNoVencido);
        }
        pagar_a_liroja(e, &pedido);
        pedido.estado = Estado::Entregado;
        guardar(e, id, &pedido);
    }

    // ----- Funciones del hotel -----

    /// El hotel hace un pedido y paga. El dinero queda guardado en el contrato.
    #[when_not_paused]
    pub fn crear_pedido(e: &Env, hotel: Address, vino: Symbol, botellas: u32) -> u32 {
        hotel.require_auth();
        if botellas == 0 {
            panic_with_error!(e, Error::CantidadInvalida);
        }
        let precio: i128 = e
            .storage()
            .persistent()
            .get(&DataKey::Precio(vino.clone()))
            .unwrap_or_else(|| panic_with_error!(e, Error::VinoNoExiste));
        let total = precio * botellas as i128;

        // Pago del hotel hacia el contrato (garantía).
        token_client(e).transfer(&hotel, &e.current_contract_address(), &total);

        let id: u32 = e.storage().instance().get(&DataKey::SiguienteId).unwrap();
        e.storage().instance().set(&DataKey::SiguienteId, &(id + 1));

        let pedido =
            Pedido { hotel: hotel.clone(), vino, botellas, total, estado: Estado::Pagado, fecha_envio: 0 };
        e.storage().persistent().set(&DataKey::Pedido(id), &pedido);
        PedidoCreado { id, hotel, total }.publish(e);
        id
    }

    /// El hotel confirma que recibió las botellas. Liroja recibe el pago.
    pub fn confirmar_recibido(e: &Env, hotel: Address, id: u32) {
        hotel.require_auth();
        let mut pedido = leer_pedido(e, id);
        if pedido.hotel != hotel {
            panic_with_error!(e, Error::NoEsElHotel);
        }
        if pedido.estado != Estado::Enviado {
            panic_with_error!(e, Error::EstadoIncorrecto);
        }
        pagar_a_liroja(e, &pedido);
        pedido.estado = Estado::Entregado;
        guardar(e, id, &pedido);
    }

    /// El hotel cancela un pedido que aún no fue enviado y recupera su dinero.
    pub fn cancelar_pedido(e: &Env, hotel: Address, id: u32) {
        hotel.require_auth();
        let mut pedido = leer_pedido(e, id);
        if pedido.hotel != hotel {
            panic_with_error!(e, Error::NoEsElHotel);
        }
        if pedido.estado != Estado::Pagado {
            panic_with_error!(e, Error::EstadoIncorrecto);
        }
        token_client(e).transfer(&e.current_contract_address(), &pedido.hotel, &pedido.total);
        pedido.estado = Estado::Cancelado;
        guardar(e, id, &pedido);
    }

    // ----- Consultas -----

    pub fn ver_pedido(e: &Env, id: u32) -> Pedido {
        leer_pedido(e, id)
    }

    pub fn ver_precio(e: &Env, vino: Symbol) -> Option<i128> {
        e.storage().persistent().get(&DataKey::Precio(vino))
    }
}

// ---------- Ayudantes internos ----------

fn token_client(e: &Env) -> token::Client<'_> {
    let token: Address = e.storage().instance().get(&DataKey::Token).unwrap();
    token::Client::new(e, &token)
}

fn leer_pedido(e: &Env, id: u32) -> Pedido {
    e.storage()
        .persistent()
        .get(&DataKey::Pedido(id))
        .unwrap_or_else(|| panic_with_error!(e, Error::PedidoNoExiste))
}

fn guardar(e: &Env, id: u32, pedido: &Pedido) {
    e.storage().persistent().set(&DataKey::Pedido(id), pedido);
    PedidoActualizado { id, estado: pedido.estado }.publish(e);
}

fn pagar_a_liroja(e: &Env, pedido: &Pedido) {
    let dueno = ownable::get_owner(e).unwrap();
    token_client(e).transfer(&e.current_contract_address(), &dueno, &pedido.total);
}

// ---------- OpenZeppelin: Ownable y Pausable ----------

#[contractimpl(contracttrait)]
impl Ownable for PedidosHotel {}

#[contractimpl]
impl Pausable for PedidosHotel {
    fn paused(e: &Env) -> bool {
        pausable::paused(e)
    }

    #[only_owner]
    fn pause(e: &Env, _caller: Address) {
        pausable::pause(e);
    }

    #[only_owner]
    fn unpause(e: &Env, _caller: Address) {
        pausable::unpause(e);
    }
}
