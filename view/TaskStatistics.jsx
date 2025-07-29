import { Card, CardContent } from "@material-ui/core";
import { PieChart, Pie, Cell } from 'recharts';

const data = [{ name: 'Completed', value: 20 }, { name: 'In progress', value: 50 }, { name: 'Not started', value: 30 }];
const COLORS = ['#0088FE', '#00C49F', '#FFBB28'];

export const TaskStatistics = () => {
    return (
        <Card>
            <CardContent>
                <PieChart width={400} height={400}>
                    <Pie data={data} cx={200} cy={200} outerRadius={80} fill="#8884d8" label>
                        {
                            data.map((entry, index) => <Cell key={`cell-${index}`} fill={COLORS[index % COLORS.length]} />)
                        }
                    </Pie>
                </PieChart>
            </CardContent>
        </Card>
    );
};
