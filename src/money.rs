use std::convert::TryInto;

use rust_decimal::Decimal;

pub(crate) static CHF: [u8; 3] = [0x43, 0x48, 0x46];

#[derive(Clone, Copy)]
pub struct Money {
    pub currency: [u8; 3],
    pub amount: Decimal,
}

impl Money {
    pub fn new(currency: &str, amount: Decimal) -> Self {
        Self {
            currency: currency.as_bytes().try_into().unwrap(),
            amount,
        }
    }
}

impl std::fmt::Debug for Money {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Money")
            .field(
                "currency",
                &String::from_utf8(self.currency.to_vec()).unwrap(),
            )
            .field("amount", &self.amount)
            .finish()
    }
}

impl std::fmt::Display for Money {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {}",
            std::str::from_utf8(&self.currency).unwrap(),
            self.amount
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    #[test]
    fn compensate_rounding() {
        let share_price = Money::new("CHF", Decimal::from_str("2711.97").unwrap());
        let valuta = Money::new("CHF", Decimal::from_str("41.53").unwrap());
        let shares = valuta.amount / share_price.amount;

        assert_eq!(shares.round_dp(7), Decimal::from_str("0.0153136").unwrap());
        assert_eq!(
            (shares.round_dp(7) * share_price.amount).round_dp(2),
            valuta.amount
        );
    }

    #[test]
    fn compensate_rounding_5digits() {
        // first compute number of shares to 5 digit precision
        let share_price = Money::new("USD", Decimal::from_str("29.39").unwrap());
        let valuta = Money::new("USD", Decimal::from_str("16.14").unwrap());
        let shares = (valuta.amount / share_price.amount).round_dp(9);
        // use that to compute the share-price with 5 digit precision
        // use that new share-price to compute the final amount of shares
        assert_eq!(shares, Decimal::from_str("0.549166383").unwrap()); // pdf says 0.549
        let share_price_fake = valuta.amount / shares.round_dp(5);
        let shares = (valuta.amount / share_price_fake.round_dp(5)).round_dp(9);
        assert_eq!(shares, Decimal::from_str("0.549169933").unwrap());
        assert_eq!((shares * share_price.amount).round_dp(2), valuta.amount);
        // Same stock but different point in time
        let share_price = Money::new("CHF", Decimal::from_str("31.27").unwrap());
        let valuta = Money::new("CHF", Decimal::from_str("17.17").unwrap());
        let later_shares = (valuta.amount / share_price.amount).round_dp(9);
        let share_price_fake = valuta.amount / later_shares.round_dp(5);
        let shares = (valuta.amount / share_price_fake.round_dp(5)).round_dp(9);
        assert_eq!(later_shares, shares);
        //assert_eq!(shares, Decimal::from_str("0.54917").unwrap()); // pdf says 0.549
        assert_eq!((shares * share_price.amount).round_dp(2), valuta.amount);
    }
}
