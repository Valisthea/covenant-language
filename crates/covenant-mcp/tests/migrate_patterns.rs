use covenant_mcp::migrate;

#[test]
fn converts_uint256_to_amount() {
    let sol = "contract Foo { uint256 balance; }";
    let result = migrate::apply(sol);
    assert!(result.output.contains("amount"), "uint256 not converted");
    assert!(!result.output.contains("uint256"), "uint256 still present");
}

#[test]
fn converts_msg_sender_to_caller() {
    let sol =
        "contract Foo { function who() public view returns (address) { return msg.sender; } }";
    let result = migrate::apply(sol);
    assert!(result.output.contains("caller"), "msg.sender not converted");
    assert!(
        !result.output.contains("msg.sender"),
        "msg.sender still present"
    );
}

#[test]
fn converts_comment_syntax() {
    let sol = "// This is a comment\ncontract Foo {}";
    let result = migrate::apply(sol);
    assert!(result.output.contains("--"), "// not converted to --");
}

#[test]
fn converts_string_to_text() {
    let sol = "contract Foo { string name; }";
    let result = migrate::apply(sol);
    assert!(
        result.output.contains("text"),
        "string not converted to text"
    );
}

#[test]
fn converts_mapping_to_map() {
    let sol = "contract Foo { mapping(address => uint256) public balances; }";
    let result = migrate::apply(sol);
    assert!(result.output.contains("map<"), "mapping not converted");
    assert!(
        !result.output.contains("mapping("),
        "mapping(...) still present"
    );
}

#[test]
fn classifies_erc20_as_token() {
    let sol = r#"
        contract MyToken {
            function transfer(address to, uint256 value) public returns (bool) {}
            function approve(address spender, uint256 value) public returns (bool) {}
            function balanceOf(address account) public view returns (uint256) {}
        }
    "#;
    let result = migrate::apply(sol);
    assert_eq!(result.construct, "token");
}

#[test]
fn classifies_vault_pattern() {
    let sol = r#"
        contract Vault is ReentrancyGuard {
            function withdraw(uint256 amount) public nonReentrant {
                payable(msg.sender).transfer(amount);
            }
        }
    "#;
    let result = migrate::apply(sol);
    assert_eq!(result.construct, "vault");
}

#[test]
fn applied_list_is_non_empty() {
    let sol = "contract Foo { uint256 x; function bar() public { x = 1; } }";
    let result = migrate::apply(sol);
    assert!(!result.applied.is_empty(), "no transformations applied");
}
