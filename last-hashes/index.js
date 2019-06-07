const Web3 = require('web3');
const fs = require('fs');

let web3 = new Web3(new Web3.providers.WebsocketProvider('wss://mainnet.infura.io/ws/v3/188460d305aa412c8443355733c2bea6'));

let eth = web3.eth;

let beginBlock = 7840001;

async function getLastHashes(blockNumber, number) {
    let blocks = [];
    for (i = 1; i <= number; i++){
        blocks.push(eth.getBlock(blockNumber - i));
    }
    blocks = await Promise.all(blocks);
    let lastHashes = blocks.map(block => block.hash);
    return lastHashes;
}

const dir = "../res/lastHashes" + beginBlock.toString();

let number = 256;
getLastHashes(beginBlock, number).then((lastHashes) => {
    console.log(lastHashes);
    for (let i = 0; i < number; i++) {
        fs.appendFileSync(dir, lastHashes[i]+'\n');
    }
    process.exit();
});
