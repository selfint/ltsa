# ltsa
Static analysis tools leveraging LSP and tree-sitter.

## Example output

Executed on [contract](https://github.com/selfint/ltsa/tree/main/tools/scanexr/tests/solidity/contract)

<div id="stacktrace" style="display: block;"><div>
                        <h1>Stacktrace: 1</h1>
                        <div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2>  1. <a href="#" onclick="return setFile('other_file.sol');">
                        other_file.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 3
                "><span>function other(address a, uint b) pure returns (uint, address) {</span>
<span>    return (b, a);</span>
<span>}</span>
<span></span>
<span>function hacky(address target, uint amount) {</span>
<span>    (bool sent, ) = <mark>target</mark>.call{value: amount}("");</span>
<span>    /*              ^^^^^^ start */</span>
<span>    require(sent, "Failed to send Ether");</span>
<span>}</span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2>  2. <a href="#" onclick="return setFile('other_file.sol');">
                        other_file.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 3
                "><span>function other(address a, uint b) pure returns (uint, address) {</span>
<span>    return (b, a);</span>
<span>}</span>
<span></span>
<span>function hacky(address target, uint amount) {</span>
<span>    (bool sent, ) = <mark>target</mark>.call{value: amount}("");</span>
<span>    /*              ^^^^^^ start */</span>
<span>    require(sent, "Failed to send Ether");</span>
<span>}</span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2>  3. <a href="#" onclick="return setFile('other_file.sol');">
                        other_file.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 3
                "><span>function other(address a, uint b) pure returns (uint, address) {</span>
<span>    return (b, a);</span>
<span>}</span>
<span></span>
<span>function hacky(address target, uint amount) {</span>
<span>    (bool sent, ) = <mark>target</mark>.call{value: amount}("");</span>
<span>    /*              ^^^^^^ start */</span>
<span>    require(sent, "Failed to send Ether");</span>
<span>}</span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2>  4. <a href="#" onclick="return setFile('other_file.sol');">
                        other_file.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 2
                "><span></span>
<span>function other(address a, uint b) pure returns (uint, address) {</span>
<span>    return (b, a);</span>
<span>}</span>
<span></span>
<span>function hacky(address <mark>target</mark>, uint amount) {</span>
<span>    (bool sent, ) = target.call{value: amount}("");</span>
<span>    /*              ^^^^^^ start */</span>
<span>    require(sent, "Failed to send Ether");</span>
<span>}</span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2>  5. <a href="#" onclick="return setFile('other_file.sol');">
                        other_file.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 2
                "><span></span>
<span>function other(address a, uint b) pure returns (uint, address) {</span>
<span>    return (b, a);</span>
<span>}</span>
<span></span>
<span>function <mark>hacky</mark>(address target, uint amount) {</span>
<span>    (bool sent, ) = target.call{value: amount}("");</span>
<span>    /*              ^^^^^^ start */</span>
<span>    require(sent, "Failed to send Ether");</span>
<span>}</span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2>  6. <a href="#" onclick="return setFile('contract.sol');">
                        contract.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 69
                "><span>        address bar = foo(sender, sender);</span>
<span></span>
<span>        uint bal = balances[bar];</span>
<span>        require(bal &gt; 0);</span>
<span></span>
<span>        <mark>hacky</mark>(bar, bal);</span>
<span></span>
<span>        balances[bar] = 0;</span>
<span>    }</span>
<span></span>
<span>    // Helper function to check the balance of this contract</span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2>  7. <a href="#" onclick="return setFile('contract.sol');">
                        contract.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 69
                "><span>        address bar = foo(sender, sender);</span>
<span></span>
<span>        uint bal = balances[bar];</span>
<span>        require(bal &gt; 0);</span>
<span></span>
<span>        hacky(<mark>bar</mark>, bal);</span>
<span></span>
<span>        balances[bar] = 0;</span>
<span>    }</span>
<span></span>
<span>    // Helper function to check the balance of this contract</span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2>  8. <a href="#" onclick="return setFile('contract.sol');">
                        contract.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 69
                "><span>        address bar = foo(sender, sender);</span>
<span></span>
<span>        uint bal = balances[bar];</span>
<span>        require(bal &gt; 0);</span>
<span></span>
<span>        hacky(<mark>bar</mark>, bal);</span>
<span></span>
<span>        balances[bar] = 0;</span>
<span>    }</span>
<span></span>
<span>    // Helper function to check the balance of this contract</span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2>  9. <a href="#" onclick="return setFile('contract.sol');">
                        contract.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 64
                "><span>    }</span>
<span></span>
<span>    function withdraw() public {</span>
<span>        address sender = getSender();</span>
<span></span>
<span>        address <mark>bar</mark> = foo(sender, sender);</span>
<span></span>
<span>        uint bal = balances[bar];</span>
<span>        require(bal &gt; 0);</span>
<span></span>
<span>        hacky(bar, bal);</span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2> 10. <a href="#" onclick="return setFile('contract.sol');">
                        contract.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 64
                "><span>    }</span>
<span></span>
<span>    function withdraw() public {</span>
<span>        address sender = getSender();</span>
<span></span>
<span>        <mark>address bar</mark> = foo(sender, sender);</span>
<span></span>
<span>        uint bal = balances[bar];</span>
<span>        require(bal &gt; 0);</span>
<span></span>
<span>        hacky(bar, bal);</span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2> 11. <a href="#" onclick="return setFile('contract.sol');">
                        contract.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 64
                "><span>    }</span>
<span></span>
<span>    function withdraw() public {</span>
<span>        address sender = getSender();</span>
<span></span>
<span>        address bar = <mark>foo(sender, sender)</mark>;</span>
<span></span>
<span>        uint bal = balances[bar];</span>
<span>        require(bal &gt; 0);</span>
<span></span>
<span>        hacky(bar, bal);</span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2> 12. <a href="#" onclick="return setFile('contract.sol');">
                        contract.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 64
                "><span>    }</span>
<span></span>
<span>    function withdraw() public {</span>
<span>        address sender = getSender();</span>
<span></span>
<span>        address bar = <mark>foo</mark>(sender, sender);</span>
<span></span>
<span>        uint bal = balances[bar];</span>
<span>        require(bal &gt; 0);</span>
<span></span>
<span>        hacky(bar, bal);</span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2> 13. <a href="#" onclick="return setFile('contract.sol');">
                        contract.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 57
                "><span>        (uint c, address d) = other(a2, 1);</span>
<span></span>
<span>        return d;</span>
<span>    }</span>
<span></span>
<span>    function <mark>foo</mark>(address a, address b) private pure returns (address) {</span>
<span>        return a;</span>
<span>    }</span>
<span></span>
<span>    function withdraw() public {</span>
<span>        address sender = getSender();</span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2> 14. <a href="#" onclick="return setFile('contract.sol');">
                        contract.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 58
                "><span></span>
<span>        return d;</span>
<span>    }</span>
<span></span>
<span>    function foo(address a, address b) private pure returns (address) {</span>
<span>        return <mark>a</mark>;</span>
<span>    }</span>
<span></span>
<span>    function withdraw() public {</span>
<span>        address sender = getSender();</span>
<span></span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2> 15. <a href="#" onclick="return setFile('contract.sol');">
                        contract.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 58
                "><span></span>
<span>        return d;</span>
<span>    }</span>
<span></span>
<span>    function foo(address a, address b) private pure returns (address) {</span>
<span>        return <mark>a</mark>;</span>
<span>    }</span>
<span></span>
<span>    function withdraw() public {</span>
<span>        address sender = getSender();</span>
<span></span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2> 16. <a href="#" onclick="return setFile('contract.sol');">
                        contract.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 57
                "><span>        (uint c, address d) = other(a2, 1);</span>
<span></span>
<span>        return d;</span>
<span>    }</span>
<span></span>
<span>    function foo(address <mark>a</mark>, address b) private pure returns (address) {</span>
<span>        return a;</span>
<span>    }</span>
<span></span>
<span>    function withdraw() public {</span>
<span>        address sender = getSender();</span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2> 17. <a href="#" onclick="return setFile('contract.sol');">
                        contract.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 64
                "><span>    }</span>
<span></span>
<span>    function withdraw() public {</span>
<span>        address sender = getSender();</span>
<span></span>
<span>        address bar = <mark>foo</mark>(sender, sender);</span>
<span></span>
<span>        uint bal = balances[bar];</span>
<span>        require(bal &gt; 0);</span>
<span></span>
<span>        hacky(bar, bal);</span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2> 18. <a href="#" onclick="return setFile('contract.sol');">
                        contract.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 64
                "><span>    }</span>
<span></span>
<span>    function withdraw() public {</span>
<span>        address sender = getSender();</span>
<span></span>
<span>        address bar = foo(<mark>sender</mark>, sender);</span>
<span></span>
<span>        uint bal = balances[bar];</span>
<span>        require(bal &gt; 0);</span>
<span></span>
<span>        hacky(bar, bal);</span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2> 19. <a href="#" onclick="return setFile('contract.sol');">
                        contract.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 64
                "><span>    }</span>
<span></span>
<span>    function withdraw() public {</span>
<span>        address sender = getSender();</span>
<span></span>
<span>        address bar = foo(<mark>sender</mark>, sender);</span>
<span></span>
<span>        uint bal = balances[bar];</span>
<span>        require(bal &gt; 0);</span>
<span></span>
<span>        hacky(bar, bal);</span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2> 20. <a href="#" onclick="return setFile('contract.sol');">
                        contract.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 62
                "><span>    function foo(address a, address b) private pure returns (address) {</span>
<span>        return a;</span>
<span>    }</span>
<span></span>
<span>    function withdraw() public {</span>
<span>        address <mark>sender</mark> = getSender();</span>
<span></span>
<span>        address bar = foo(sender, sender);</span>
<span></span>
<span>        uint bal = balances[bar];</span>
<span>        require(bal &gt; 0);</span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2> 21. <a href="#" onclick="return setFile('contract.sol');">
                        contract.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 62
                "><span>    function foo(address a, address b) private pure returns (address) {</span>
<span>        return a;</span>
<span>    }</span>
<span></span>
<span>    function withdraw() public {</span>
<span>        <mark>address sender</mark> = getSender();</span>
<span></span>
<span>        address bar = foo(sender, sender);</span>
<span></span>
<span>        uint bal = balances[bar];</span>
<span>        require(bal &gt; 0);</span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2> 22. <a href="#" onclick="return setFile('contract.sol');">
                        contract.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 62
                "><span>    function foo(address a, address b) private pure returns (address) {</span>
<span>        return a;</span>
<span>    }</span>
<span></span>
<span>    function withdraw() public {</span>
<span>        address sender = <mark>getSender()</mark>;</span>
<span></span>
<span>        address bar = foo(sender, sender);</span>
<span></span>
<span>        uint bal = balances[bar];</span>
<span>        require(bal &gt; 0);</span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2> 23. <a href="#" onclick="return setFile('contract.sol');">
                        contract.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 62
                "><span>    function foo(address a, address b) private pure returns (address) {</span>
<span>        return a;</span>
<span>    }</span>
<span></span>
<span>    function withdraw() public {</span>
<span>        address sender = <mark>getSender</mark>();</span>
<span></span>
<span>        address bar = foo(sender, sender);</span>
<span></span>
<span>        uint bal = balances[bar];</span>
<span>        require(bal &gt; 0);</span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2> 24. <a href="#" onclick="return setFile('contract.sol');">
                        contract.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 39
                "><span>    function getSender2() private view returns (address) {</span>
<span>        return msg.sender;</span>
<span>        /*     ^^^^^^^^^^ definition */</span>
<span>    }</span>
<span></span>
<span>    function <mark>getSender</mark>() private view returns (address) {</span>
<span>        if (false) {</span>
<span>            return foo(0x0000000000000000000000000000000000000000, msg.sender);</span>
<span>            /*                                                     ^^^^^^^^^^ definition */</span>
<span>        } else if (false) {</span>
<span>            return ret2(0x0000000000000000000000000000000000000000, msg.sender);</span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2> 25. <a href="#" onclick="return setFile('contract.sol');">
                        contract.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 41
                "><span>        /*     ^^^^^^^^^^ definition */</span>
<span>    }</span>
<span></span>
<span>    function getSender() private view returns (address) {</span>
<span>        if (false) {</span>
<span>            return <mark>foo(0x0000000000000000000000000000000000000000, msg.sender)</mark>;</span>
<span>            /*                                                     ^^^^^^^^^^ definition */</span>
<span>        } else if (false) {</span>
<span>            return ret2(0x0000000000000000000000000000000000000000, msg.sender);</span>
<span>            /*                                                      ^^^^^^^^^^ definition */</span>
<span>        }</span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2> 26. <a href="#" onclick="return setFile('contract.sol');">
                        contract.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 41
                "><span>        /*     ^^^^^^^^^^ definition */</span>
<span>    }</span>
<span></span>
<span>    function getSender() private view returns (address) {</span>
<span>        if (false) {</span>
<span>            return <mark>foo</mark>(0x0000000000000000000000000000000000000000, msg.sender);</span>
<span>            /*                                                     ^^^^^^^^^^ definition */</span>
<span>        } else if (false) {</span>
<span>            return ret2(0x0000000000000000000000000000000000000000, msg.sender);</span>
<span>            /*                                                      ^^^^^^^^^^ definition */</span>
<span>        }</span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2> 27. <a href="#" onclick="return setFile('contract.sol');">
                        contract.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 57
                "><span>        (uint c, address d) = other(a2, 1);</span>
<span></span>
<span>        return d;</span>
<span>    }</span>
<span></span>
<span>    function <mark>foo</mark>(address a, address b) private pure returns (address) {</span>
<span>        return a;</span>
<span>    }</span>
<span></span>
<span>    function withdraw() public {</span>
<span>        address sender = getSender();</span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2> 28. <a href="#" onclick="return setFile('contract.sol');">
                        contract.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 58
                "><span></span>
<span>        return d;</span>
<span>    }</span>
<span></span>
<span>    function foo(address a, address b) private pure returns (address) {</span>
<span>        return <mark>a</mark>;</span>
<span>    }</span>
<span></span>
<span>    function withdraw() public {</span>
<span>        address sender = getSender();</span>
<span></span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2> 29. <a href="#" onclick="return setFile('contract.sol');">
                        contract.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 58
                "><span></span>
<span>        return d;</span>
<span>    }</span>
<span></span>
<span>    function foo(address a, address b) private pure returns (address) {</span>
<span>        return <mark>a</mark>;</span>
<span>    }</span>
<span></span>
<span>    function withdraw() public {</span>
<span>        address sender = getSender();</span>
<span></span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2> 30. <a href="#" onclick="return setFile('contract.sol');">
                        contract.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 57
                "><span>        (uint c, address d) = other(a2, 1);</span>
<span></span>
<span>        return d;</span>
<span>    }</span>
<span></span>
<span>    function foo(address <mark>a</mark>, address b) private pure returns (address) {</span>
<span>        return a;</span>
<span>    }</span>
<span></span>
<span>    function withdraw() public {</span>
<span>        address sender = getSender();</span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2> 31. <a href="#" onclick="return setFile('contract.sol');">
                        contract.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 41
                "><span>        /*     ^^^^^^^^^^ definition */</span>
<span>    }</span>
<span></span>
<span>    function getSender() private view returns (address) {</span>
<span>        if (false) {</span>
<span>            return <mark>foo</mark>(0x0000000000000000000000000000000000000000, msg.sender);</span>
<span>            /*                                                     ^^^^^^^^^^ definition */</span>
<span>        } else if (false) {</span>
<span>            return ret2(0x0000000000000000000000000000000000000000, msg.sender);</span>
<span>            /*                                                      ^^^^^^^^^^ definition */</span>
<span>        }</span></pre>
                    </div><div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2> 32. <a href="#" onclick="return setFile('contract.sol');">
                        contract.sol
                    </a></h2>
                        </div>
                        <pre style="
                white-space: pre-wrap;
                counter-set: line 41
                "><span>        /*     ^^^^^^^^^^ definition */</span>
<span>    }</span>
<span></span>
<span>    function getSender() private view returns (address) {</span>
<span>        if (false) {</span>
<span>            return foo(<mark>0x0000000000000000000000000000000000000000</mark>, msg.sender);</span>
<span>            /*                                                     ^^^^^^^^^^ definition */</span>
<span>        } else if (false) {</span>
<span>            return ret2(0x0000000000000000000000000000000000000000, msg.sender);</span>
<span>            /*                                                      ^^^^^^^^^^ definition */</span>
<span>        }</span></pre>
                    </div>
                    </div></div>
