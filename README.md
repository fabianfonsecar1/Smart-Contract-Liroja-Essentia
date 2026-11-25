# 🍷 Liroja Essentia – Pedidos seguros para hoteles en Stellar

Contrato inteligente en **Stellar (Soroban)** para que un hotel compre vinos artesanales de frutas a **Liroja Essentia** sin que ninguna de las dos partes corra riesgo con el pago.

---

## 1. El problema

Liroja Essentia es un emprendimiento costarricense que hace **vinos artesanales de frutas** (mora, maracuyá, piña…). Sus clientes son **hoteles de alta gama en zonas turísticas** y turistas.

Cuando un hotel hace un pedido grande aparece un problema de confianza:

- **El hotel** no quiere pagar por adelantado sin saber si el vino va a llegar.
- **Liroja Essentia**, como emprendimiento pequeño, no puede producir y enviar 50 botellas sin tener el pago asegurado.

Hoy esto se resuelve con adelantos, facturas a 30 días o mucha confianza. Eso frena las ventas y afecta el flujo de caja.

## 2. La solución

El contrato funciona como un **intermediario de confianza (garantía o *escrow*)**:

1. El hotel hace el pedido y **paga**. El dinero queda **guardado en el contrato**, no lo tiene nadie todavía.
2. Liroja Essentia ve que el pago está asegurado y **envía las botellas**.
3. El hotel **confirma que las recibió** y el contrato le paga a Liroja Essentia automáticamente.

Reglas extra para que sea justo para ambos:

- Si Liroja **todavía no envió** el pedido, el hotel puede **cancelar** y recibe su dinero de vuelta.
- Si el hotel recibe el vino pero **no confirma en 7 días**, Liroja puede **cobrar** igual.
- Solo Liroja Essentia (el dueño) pone los **precios**. El hotel no puede inventar precios.
- Liroja puede **pausar** el contrato (por ejemplo, si se acaba la cosecha) y no se aceptan pedidos nuevos.

## 3. ¿Por qué elegí este problema?

Es un problema **real, pequeño y de Costa Rica**: muchos emprendimientos locales le venden al sector turismo y siempre chocan con el mismo tema de "¿quién paga primero?". Un contrato inteligente lo resuelve con reglas claras, sin abogados ni intermediarios, y con pagos rápidos y baratos en Stellar (por ejemplo, en USDC).

## 4. Cómo funciona

```
 Hotel                       Contrato                       Liroja Essentia
   |  crear_pedido + pago  -->  [Pagado]  (dinero guardado)
   |                             |   <-- marcar_enviado           |
   |                          [Enviado]                           |
   |  confirmar_recibido   -->  [Entregado] -- paga el total -->  |
```

| Función | ¿Quién la usa? | ¿Qué hace? |
|---|---|---|
| `poner_precio(vino, precio)` | Liroja | Agrega o cambia el precio por botella de un vino |
| `crear_pedido(hotel, vino, botellas)` | Hotel | Crea el pedido y deposita el pago en el contrato |
| `marcar_enviado(id)` | Liroja | Indica que las botellas ya salieron |
| `confirmar_recibido(hotel, id)` | Hotel | Confirma la entrega y libera el pago a Liroja |
| `cancelar_pedido(hotel, id)` | Hotel | Cancela antes del envío y recupera su dinero |
| `cobrar_sin_confirmacion(id)` | Liroja | Cobra si pasaron 7 días del envío sin confirmación |
| `ver_pedido(id)` / `ver_precio(vino)` | Cualquiera | Consultas |
| `pause` / `unpause` | Liroja | Detiene o reactiva los pedidos nuevos |

**Estados de un pedido:** `Pagado` → `Enviado` → `Entregado` (o `Cancelado`).

### Librerías de OpenZeppelin usadas
Basado en [OpenZeppelin Stellar Contracts](https://github.com/OpenZeppelin/stellar-contracts) (v0.8.0):

- **`stellar-access` → Ownable:** define a Liroja Essentia como dueña. Con `#[only_owner]` solo ella puede poner precios, marcar envíos y pausar.
- **`stellar-contract-utils` → Pausable:** con `#[when_not_paused]` se bloquean pedidos nuevos cuando el contrato está en pausa.
- **`stellar-macros`:** los macros `#[only_owner]` y `#[when_not_paused]`.

## 5. Estructura

```
liroja-essentia/
├── Cargo.toml
└── contracts/pedidos-hotel/
    ├── Cargo.toml
    └── src/
        ├── lib.rs
        ├── contract.rs   # el contrato
        └── test.rs       # las pruebas
```

## 6. Cómo correrlo

**Requisitos:** [Rust](https://www.rust-lang.org/tools/install) y [Stellar CLI](https://developers.stellar.org/docs/build/smart-contracts/getting-started/setup).

```bash
rustup target add wasm32v1-none
```

**Correr las pruebas:**
```bash
cargo test
```

**Compilar el contrato:**
```bash
stellar contract build
```

**Desplegar en testnet (opcional):**
```bash
# Crear cuentas de prueba con fondos
stellar keys generate liroja --network testnet --fund
stellar keys generate hotel  --network testnet --fund

# Dirección del token XLM nativo en testnet
TOKEN=$(stellar contract id asset --asset native --network testnet)

# Desplegar (Liroja es la dueña)
stellar contract deploy \
  --wasm target/wasm32v1-none/release/pedidos_hotel.wasm \
  --source liroja --network testnet --alias liroja \
  -- --dueno liroja --token $TOKEN

# Liroja pone el precio: vino de mora a 25 XLM (7 decimales)
stellar contract invoke --id liroja --source liroja --network testnet \
  -- poner_precio --vino mora --precio 250000000

# El hotel pide 12 botellas (paga 300 XLM al contrato)
stellar contract invoke --id liroja --source hotel --network testnet \
  -- crear_pedido --hotel hotel --vino mora --botellas 12

# Liroja envía y el hotel confirma
stellar contract invoke --id liroja --source liroja --network testnet -- marcar_enviado --id 1
stellar contract invoke --id liroja --source hotel  --network testnet -- confirmar_recibido --hotel hotel --id 1

# Ver el pedido
stellar contract invoke --id liroja --source hotel --network testnet -- ver_pedido --id 1
```

## 7. Pruebas

Archivo `contracts/pedidos-hotel/src/test.rs`. Resultado de `cargo test`: **8 pruebas, 8 correctas.**

| Prueba | Qué verifica |
|---|---|
| `flujo_completo_pedido_entregado` | El hotel paga $300, Liroja envía, el hotel confirma y Liroja recibe los $300 |
| `hotel_cancela_antes_del_envio` | El hotel cancela y recupera todo su dinero |
| `no_se_puede_cancelar_si_ya_se_envio` | Después del envío ya no se puede cancelar |
| `no_se_puede_pedir_un_vino_que_no_existe` | Solo se piden vinos del catálogo |
| `otro_hotel_no_puede_confirmar` | Solo el hotel dueño del pedido puede confirmarlo |
| `liroja_cobra_si_el_hotel_no_confirma_en_7_dias` | Liroja cobra tras 7 días sin confirmación |
| `liroja_no_puede_cobrar_antes_de_7_dias` | Liroja no puede cobrar antes de tiempo |
| `no_se_aceptan_pedidos_en_pausa` | Con el contrato en pausa no entran pedidos |

### Códigos de error
| # | Error | Significado |
|---|---|---|
| 1 | `VinoNoExiste` | El vino no está en el catálogo |
| 2 | `CantidadInvalida` | Se pidieron 0 botellas |
| 3 | `PrecioInvalido` | Precio igual o menor a 0 |
| 4 | `PedidoNoExiste` | No hay pedido con ese número |
| 5 | `EstadoIncorrecto` | La acción no se puede hacer en el estado actual |
| 6 | `NoEsElHotel` | Quien llama no es el hotel del pedido |
| 7 | `PlazoNoVencido` | Aún no pasan los 7 días |

---

> Proyecto individual de práctica. No es la idea para la hackathon.
