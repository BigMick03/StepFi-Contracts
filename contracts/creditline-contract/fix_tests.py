import sys

file_path = r'c:\Users\isaac\Documents\GitHub\StepFi-Contracts\contracts\creditline-contract\src\tests.rs'
with open(file_path, 'r', encoding='utf-8') as f:
    content = f.read()

# Conflict 1
c1 = '''<<<<<<< HEAD
    pub fn liquidate_funds(env: Env, _from: Address, _lost_principal: i128, amount: i128) {
        env.storage().instance().set(&symbol_short!("GUARD"), &true);
        env.storage()
            .instance()
            .set(&symbol_short!("GUAMT"), &amount);
=======
    pub fn absorb_loss(env: Env, _from: Address, amount: i128) {
        env.storage()
            .instance()
            .set(&symbol_short!("LOSSRD"), &true);
        env.storage()
            .instance()
            .set(&symbol_short!("LOSSAM"), &amount);
>>>>>>> 35c578893272dc7795b291e8c64db7831a3da984'''

r1 = '''    pub fn liquidate_funds(env: Env, _from: Address, _lost_principal: i128, amount: i128) {
        env.storage().instance().set(&symbol_short!("GUARD"), &true);
        env.storage()
            .instance()
            .set(&symbol_short!("GUAMT"), &amount);
    }

    pub fn absorb_loss(env: Env, _from: Address, amount: i128) {
        env.storage()
            .instance()
            .set(&symbol_short!("LOSSRD"), &true);
        env.storage()
            .instance()
            .set(&symbol_short!("LOSSAM"), &amount);
    }'''

content = content.replace(c1, r1)

c1_b = '''    }    pub fn receive_guarantee(env: Env, _from: Address, amount: i128) {
        env.storage()
            .instance()
            .set(&symbol_short!("GUARD"), &true);
        env.storage()
            .instance()
            .set(&symbol_short!("GUAMT"), &amount);
    }'''
content = content.replace(c1_b, '''    }''')

c2 = '''<<<<<<< HEAD
    pub fn was_liquidate_funds_called(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&symbol_short!("GUARD"))
            .unwrap_or(false)
    }

    pub fn get_liquidate_funds_amount(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&symbol_short!("GUAMT"))
=======
    pub fn was_receive_guarantee_called(env: Env) -> bool {
        env.storage().instance().get(&symbol_short!("GUARD")).unwrap_or(false)
    }    pub fn get_receive_guarantee_amount(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&symbol_short!("GUAMT"))
            .unwrap_or(0)
    }

    pub fn was_absorb_loss_called(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&symbol_short!("LOSSRD"))
            .unwrap_or(false)
    }

    pub fn get_absorb_loss_amount(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&symbol_short!("LOSSAM"))
>>>>>>> 35c578893272dc7795b291e8c64db7831a3da984'''

r2 = '''    pub fn was_liquidate_funds_called(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&symbol_short!("GUARD"))
            .unwrap_or(false)
    }

    pub fn get_liquidate_funds_amount(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&symbol_short!("GUAMT"))
            .unwrap_or(0)
    }

    pub fn was_absorb_loss_called(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&symbol_short!("LOSSRD"))
            .unwrap_or(false)
    }

    pub fn get_absorb_loss_amount(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&symbol_short!("LOSSAM"))'''

content = content.replace(c2, r2)

c3 = '''<<<<<<< HEAD
        pub fn liquidate_funds(_env: Env, _from: Address, _lost_principal: i128, _amount: i128) {}
=======
        pub fn receive_guarantee(_env: Env, _from: Address, _amount: i128) {}

        pub fn absorb_loss(_env: Env, _from: Address, _amount: i128) {}
>>>>>>> 35c578893272dc7795b291e8c64db7831a3da984'''

r3 = '''        pub fn liquidate_funds(_env: Env, _from: Address, _lost_principal: i128, _amount: i128) {}

        pub fn absorb_loss(_env: Env, _from: Address, _amount: i128) {}'''

content = content.replace(c3, r3)

c4 = '''<<<<<<< HEAD
=======
    // After mark_defaulted now absorbs the unrecovered principal shortfall,
    // locked_liquidity is fully released (guarantee recovered + loss absorbed = 0 locked).
>>>>>>> 35c578893272dc7795b291e8c64db7831a3da984'''

r4 = '''    // After check_default now absorbs the unrecovered principal shortfall,
    // locked_liquidity is fully released (guarantee recovered + loss absorbed = 0 locked).'''

content = content.replace(c4, r4)

c5_1 = '''<<<<<<< HEAD\n'''
c5_2 = '''=======\n'''
c5_3 = '''>>>>>>> 35c578893272dc7795b291e8c64db7831a3da984\n'''
c5_3_alt = '''>>>>>>> 35c578893272dc7795b291e8c64db7831a3da984'''

content = content.replace(c5_1, '')
content = content.replace(c5_2, '')
content = content.replace(c5_3, '')
content = content.replace(c5_3_alt, '')

content = content.replace('test_mark_defaulted', 'test_check_default')
content = content.replace('mark_defaulted', 'check_default')
content = content.replace('was_receive_guarantee_called', 'was_liquidate_funds_called')
content = content.replace('After receive_guarantee(200):', 'After liquidate_funds(200):')

content = content.replace('t.advance_past(5000);', 't.advance_past(5000 + 86400 * 30);')
content = content.replace('env.ledger().set_timestamp(5001);', 'env.ledger().set_timestamp(5000 + 86400 * 30 + 1);')

with open(file_path, 'w', encoding='utf-8') as f:
    f.write(content)

print("Python script done.")
