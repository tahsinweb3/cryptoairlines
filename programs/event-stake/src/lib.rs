use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

declare_id!("DCMGyCAY4BhZqoSvQNSzPxDAsWVpFRcjYce974hNvNCc");

#[program]
mod event_stake {
    use super::*;

    // Admin yeni bir etkinlik oluşturur.
    pub fn create_event(
        ctx: Context<CreateEvent>,
        minimum_stake: u64,
        max_participants: u64,
        duration: i64,
    ) -> Result<()> {
        let event = &mut ctx.accounts.event;
        event.admin = *ctx.accounts.admin.key;
        event.minimum_stake = minimum_stake;
        event.max_participants = max_participants;
        event.participant_count = 0;
        event.start_time = Clock::get()?.unix_timestamp;
        event.end_time = event.start_time + duration;
        event.is_active = true;
        Ok(())
    }

    // Kullanıcı token'larını kilitleyerek etkinliğe kayıt olur (stake eder ve register olur).
    pub fn stake_and_register(ctx: Context<StakeAndRegister>) -> Result<()> {
        let event = &mut ctx.accounts.event;
        let registration = &mut ctx.accounts.registration;

        // Etkinlik aktif mi, dolu mu ve kullanıcı zaten kayıtlı mı kontrolleri
        require!(event.is_active, EventError::EventInactive);
        require!(
            event.participant_count < event.max_participants,
            EventError::EventFull
        );
        require!(!registration.is_registered, EventError::AlreadyRegistered);

        // Minimum stake miktarını alıyoruz
        let amount = event.minimum_stake;

        // Kullanıcıyı etkinliğe kaydet
        registration.user = *ctx.accounts.user.key;
        registration.event = *ctx.accounts.event.as_ref();
        registration.is_registered = true;

        // Katılımcı sayısını artır
        event.participant_count += 1;

        // SPL Token transferi (stake etmek için kullanıcıdan event'e token aktarılır)
        token::transfer(ctx.accounts.into_transfer_to_event_context(), amount)?;

        Ok(())
    }

    // Admin etkinliği iptal eder.
    pub fn cancel_event(ctx: Context<CancelEvent>) -> Result<()> {
        let event = &mut ctx.accounts.event;
        require!(
            event.admin == *ctx.accounts.admin.key,
            EventError::Unauthorized
        );
        event.is_active = false;
        Ok(())
    }

    // Kullanıcı tokenlarını geri çeker (etkinlik iptal edildiğinde veya stake süresi dolduğunda).
    pub fn withdraw_tokens(ctx: Context<WithdrawTokens>) -> Result<()> {
        let event = &ctx.accounts.event;
        let current_time = Clock::get()?.unix_timestamp;

        require!(
            !event.is_active || current_time >= event.end_time,
            EventError::CannotWithdrawYet
        );

        // Minimum stake miktarını alıyoruz
        let amount = event.minimum_stake;

        // SPL Token transferi (tokenlar kullanıcıya geri gönderilir)
        token::transfer(ctx.accounts.into_transfer_to_user_context(), amount)?;

        Ok(())
    }
}

// Context'ler
#[derive(Accounts)]
pub struct CreateEvent<'info> {
    #[account(init, payer = admin, space = 8 + 32 + 8 + 8 + 8 + 8 + 1)]
    pub event: Account<'info, Event>,
    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct StakeAndRegister<'info> {
    #[account(mut)]
    pub event: Account<'info, Event>,
    #[account(init, payer = user, space = 8 + 32 + 32 + 1)]
    pub registration: Account<'info, Registration>,
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut, has_one = mint)]
    pub from: Account<'info, TokenAccount>, // Kullanıcının token hesabı
    #[account(mut)]
    pub event_vault: Account<'info, TokenAccount>, // Etkinlik için tokenların stake edileceği yer
    pub mint: Account<'info, Mint>, // SPL Token'ın mint account'ı
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CancelEvent<'info> {
    #[account(mut)]
    pub event: Account<'info, Event>,
    pub admin: Signer<'info>,
}

#[derive(Accounts)]
pub struct WithdrawTokens<'info> {
    #[account(mut)]
    pub event: Account<'info, Event>,
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut)]
    pub event_vault: Account<'info, TokenAccount>, // Etkinlikte stake edilen tokenlar burada
    #[account(mut, has_one = mint)]
    pub to: Account<'info, TokenAccount>, // Kullanıcının token hesabı
    pub mint: Account<'info, Mint>, // SPL Token'ın mint account'ı
    pub token_program: Program<'info, Token>,
}

// Event account'ı: Etkinliğin bilgilerini tutar.
#[account]
pub struct Event {
    pub admin: Pubkey,
    pub minimum_stake: u64,
    pub max_participants: u64,
    pub participant_count: u64,
    pub start_time: i64,
    pub end_time: i64,
    pub is_active: bool,
}

// Registration account'ı: Her kullanıcı için etkinliğe kayıt bilgilerini tutar.
#[account]
pub struct Registration {
    pub user: Pubkey,
    pub event: Pubkey,
    pub is_registered: bool,
}

// Hata yönetimi
#[error_code]
pub enum EventError {
    #[msg("Etkinlik aktif değil.")]
    EventInactive,
    #[msg("Etkinlik dolu.")]
    EventFull,
    #[msg("Bu etkinliğe zaten kayıt oldunuz.")]
    AlreadyRegistered,
    #[msg("Minimum stake miktarından daha az token kilitlenemez.")]
    StakeTooLow,
    #[msg("Tokenları henüz çekemezsiniz.")]
    CannotWithdrawYet,
    #[msg("Bu işlemi gerçekleştirmeye yetkili değilsiniz.")]
    Unauthorized,
}

// SPL Token transfer işlemleri için yardımcı fonksiyonlar
impl<'info> StakeAndRegister<'info> {
    pub fn into_transfer_to_event_context(&self) -> CpiContext<'_, '_, '_, 'info, Transfer<'info>> {
        let cpi_accounts = Transfer {
            from: self.from.to_account_info(),
            to: self.event_vault.to_account_info(),
            authority: self.user.to_account_info(),
        };
        CpiContext::new(self.token_program.to_account_info(), cpi_accounts)
    }
}

impl<'info> WithdrawTokens<'info> {
    pub fn into_transfer_to_user_context(&self) -> CpiContext<'_, '_, '_, 'info, Transfer<'info>> {
        let cpi_accounts = Transfer {
            from: self.event_vault.to_account_info(),
            to: self.to.to_account_info(),
            authority: self.event.to_account_info(),
        };
        CpiContext::new(self.token_program.to_account_info(), cpi_accounts)
    }
}
