extern crate std;

use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger},
    token::{StellarAssetClient, TokenClient},
    Address, Env,
};

use crate::{Estado, PedidosHotelClient, DIAS_PARA_CONFIRMAR};

// 1 dólar = 10_000_000 unidades (7 decimales, como USDC en Stellar)
const DOLAR: i128 = 10_000_000;

struct Escenario<'a> {
    e: Env,
    liroja: Address,
    hotel: Address,
    token: TokenClient<'a>,
    contrato: PedidosHotelClient<'a>,
}

fn preparar<'a>() -> Escenario<'a> {
    let e = Env::default();
    e.mock_all_auths();

    let liroja = Address::generate(&e);
    let hotel = Address::generate(&e);

    // Moneda de prueba y saldo inicial del hotel: $1000
    let emisor = Address::generate(&e);
    let sac = e.register_stellar_asset_contract_v2(emisor);
    StellarAssetClient::new(&e, &sac.address()).mint(&hotel, &(1000 * DOLAR));
    let token = TokenClient::new(&e, &sac.address());

    let id = e.register(crate::PedidosHotel, (&liroja, &sac.address()));
    let contrato = PedidosHotelClient::new(&e, &id);

    // Catálogo: vino de mora a $25 la botella
    contrato.poner_precio(&symbol_short!("mora"), &(25 * DOLAR));

    Escenario { e, liroja, hotel, token, contrato }
}

#[test]
fn flujo_completo_pedido_entregado() {
    let s = preparar();

    // El hotel pide 12 botellas => $300 quedan guardados en el contrato
    let id = s.contrato.crear_pedido(&s.hotel, &symbol_short!("mora"), &12);
    assert_eq!(s.token.balance(&s.hotel), 700 * DOLAR);
    assert_eq!(s.token.balance(&s.contrato.address), 300 * DOLAR);

    s.contrato.marcar_enviado(&id);
    s.contrato.confirmar_recibido(&s.hotel, &id);

    // Liroja recibe el pago
    assert_eq!(s.token.balance(&s.liroja), 300 * DOLAR);
    assert_eq!(s.token.balance(&s.contrato.address), 0);
    assert_eq!(s.contrato.ver_pedido(&id).estado, Estado::Entregado);
}

#[test]
fn hotel_cancela_antes_del_envio() {
    let s = preparar();
    let id = s.contrato.crear_pedido(&s.hotel, &symbol_short!("mora"), &4);

    s.contrato.cancelar_pedido(&s.hotel, &id);

    assert_eq!(s.token.balance(&s.hotel), 1000 * DOLAR);
    assert_eq!(s.contrato.ver_pedido(&id).estado, Estado::Cancelado);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn no_se_puede_cancelar_si_ya_se_envio() {
    let s = preparar();
    let id = s.contrato.crear_pedido(&s.hotel, &symbol_short!("mora"), &4);
    s.contrato.marcar_enviado(&id);

    s.contrato.cancelar_pedido(&s.hotel, &id);
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn no_se_puede_pedir_un_vino_que_no_existe() {
    let s = preparar();
    s.contrato.crear_pedido(&s.hotel, &symbol_short!("uva"), &1);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn otro_hotel_no_puede_confirmar() {
    let s = preparar();
    let id = s.contrato.crear_pedido(&s.hotel, &symbol_short!("mora"), &2);
    s.contrato.marcar_enviado(&id);

    let otro_hotel = Address::generate(&s.e);
    s.contrato.confirmar_recibido(&otro_hotel, &id);
}

#[test]
fn liroja_cobra_si_el_hotel_no_confirma_en_7_dias() {
    let s = preparar();
    let id = s.contrato.crear_pedido(&s.hotel, &symbol_short!("mora"), &2);
    s.contrato.marcar_enviado(&id);

    // Pasan 7 días sin confirmación
    s.e.ledger().with_mut(|l| l.timestamp += DIAS_PARA_CONFIRMAR * 24 * 60 * 60);
    s.contrato.cobrar_sin_confirmacion(&id);

    assert_eq!(s.token.balance(&s.liroja), 50 * DOLAR);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn liroja_no_puede_cobrar_antes_de_7_dias() {
    let s = preparar();
    let id = s.contrato.crear_pedido(&s.hotel, &symbol_short!("mora"), &2);
    s.contrato.marcar_enviado(&id);

    s.contrato.cobrar_sin_confirmacion(&id);
}

#[test]
#[should_panic]
fn no_se_aceptan_pedidos_en_pausa() {
    let s = preparar();
    s.contrato.pause(&s.liroja);
    s.contrato.crear_pedido(&s.hotel, &symbol_short!("mora"), &1);
}
