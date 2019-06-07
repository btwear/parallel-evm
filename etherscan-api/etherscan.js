const rp = require('request-promise');

const baseURL = "https://api.etherscan.io/api"

module.exports =  function(apiKey) {
    this.apiKey = apiKey;
    this.getBlockReward = async function(blockNumber) {
        let options = {
            uri: baseURL,
            qs: {
                module: "block",
                action: "getblockreward",
                blockno: blockNumber.toString(),
                apikey: this.apiKwy,
            },
            json: true
        };

        let res = await rp(options);
        let result = res.result;
        delete result.timeStamp;

        return result;
    }
};
