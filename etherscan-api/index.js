const fs = require('fs');
const Etherscan = require('./etherscan.js');
const program = require('commander');

program
    .option('-f --from <number>', 'The begin block of download', parseInt)
    .option('-t --to <number>', 'The end block of download', parseInt)
    .option('-d --directory <dir>', 'Directory to save block rewards data')
    .option('-n --filename <filename>', 'Filename');

program.parse(process.argv);
const from = program.from;
const to = program.to;
const dir = program.directory;
const filename = program.filename ? program.filename : from.toString() + '_' + to.toString() + '.json';
console.log(from, to, dir, filename)

const etherscan = new Etherscan('JM651JSRRX9FTTBHXTW4WK98RNIJCEYAQD');

const getBlockRewardSync = async (from, to) => {
    let saveDir = dir + '/' + filename;
    for(i = from; i <= to; i += 5) {
        let rewards = [];
        console.log('Processing block #' + i.toString());
        for (j = 0; j < 5; j++) {
            rewards.push(etherscan.getBlockReward(i + j));
        }
        rewards = await Promise.all(rewards);
        for (j = 0; j < 5; j++) {
            fs.appendFileSync(saveDir, JSON.stringify(rewards[j]) + '\n');
        }
    }
}

getBlockRewardSync(from, to).then(() => {
    console.log('All done');
});
