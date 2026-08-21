import type { Offering, CashTotal, Donation, GasdsResult, Denomination } from "./types";
import { DENOMINATIONS, fmtPence } from "./types";

type Counts = Record<Denomination, number>;

export function sumByPaymentMethod(
    donations: Donation[],
    method: Donation["payment_method"]
): number {
    return donations
        .filter((d) => d.payment_method === method)
        .reduce((s, d) => s + d.amount.pence, 0);
}

export function computeDonationTotals(offering: Offering | null): {
    cash: number;
    cheque: number;
    bank: number;
    card: number;
    total: number;
} {
    const dons = offering?.donations ?? [];
    const cash = sumByPaymentMethod(dons, "Cash");
    const cheque = sumByPaymentMethod(dons, "Cheque");
    const bank = sumByPaymentMethod(dons, "BankTransfer");
    const card = sumByPaymentMethod(dons, "Contactless") + sumByPaymentMethod(dons, "ChipAndPin");
    return { cash, cheque, bank, card, total: cash + cheque + bank + card };
}

export function computeDonationBreakdown(
    offering: Offering | null
): Record<string, { recorded: number; counted: number }> {
    const breakdown: Record<string, { recorded: number; counted: number }> = {};

    offering?.donations?.forEach((d) => {
        const counts = d.cash_denominations?.counts as Counts | undefined;
        counts &&
            Object.entries(counts).forEach(([denom, count]) => {
                const p = DENOMINATIONS.find((x) => x.name === denom)?.pence ?? 0;
                if (!breakdown[denom]) breakdown[denom] = { recorded: 0, counted: 0 };
                breakdown[denom].recorded += p * count;
            });
    });

    const offeringCounts = offering?.cash_denominations?.counts as Counts | undefined;
    offeringCounts &&
        Object.entries(offeringCounts).forEach(([denom, count]) => {
            const p = DENOMINATIONS.find((x) => x.name === denom)?.pence ?? 0;
            if (!breakdown[denom]) breakdown[denom] = { recorded: 0, counted: 0 };
            breakdown[denom].counted += p * count;
        });

    return breakdown;
}

export function computeDenomDiff(
    offering: Offering | null
): Record<string, number> {
    const breakdown = computeDonationBreakdown(offering);
    const diff: Record<string, number> = {};
    for (const [denom, { recorded, counted }] of Object.entries(breakdown)) {
        diff[denom] = counted - recorded;
    }
    return diff;
}

export function computeCashWithNoBreakdown(offering: Offering | null): number {
    return offering?.donations
        ?.filter((d) => d.payment_method === "Cash" && !d.cash_denominations)
        .reduce((s, d) => s + d.amount.pence, 0) ?? 0;
}

export function computeReportTotals(report: { offering: { donations: Donation[] } } | null): {
    cash: number;
    cheque: number;
    bank: number;
    card: number;
    total: number;
} {
    if (!report) return { cash: 0, cheque: 0, bank: 0, card: 0, total: 0 };
    const dons = report.offering.donations ?? [];
    const cash = sumByPaymentMethod(dons, "Cash");
    const cheque = sumByPaymentMethod(dons, "Cheque");
    const bank = sumByPaymentMethod(dons, "BankTransfer");
    const card = sumByPaymentMethod(dons, "Contactless") + sumByPaymentMethod(dons, "ChipAndPin");
    return { cash, cheque, bank, card, total: cash + cheque + bank + card };
}

export function fmtDiff(diff: number): string {
    if (diff === 0) return "";
    const sign = diff > 0 ? "+" : "";
    return `${sign}${fmtPence(diff)}`;
}

export function formatAmendmentChange(change: {
    kind: string;
    attendance?: number;
    cash_denominations?: CashTotal;
    description?: string;
    add_donation?: Donation;
    remove_donation_id?: string;
}): string {
    switch (change.kind) {
        case "SetAttendance":
            return `Attendance: ${change.attendance}`;
        case "SetCashDenominations":
            return `Cash: ${change.cash_denominations ? fmtPence(totalCash(change.cash_denominations)) : "cleared"}`;
        case "SetDescription":
            return `Description: ${change.description ?? "cleared"}`;
        case "AddDonation":
            return `Added ${fmtPence(change.add_donation?.amount.pence ?? 0)} (${change.add_donation?.donation_type ?? ""}, ${change.add_donation?.payment_method ?? ""})`;
        case "RemoveDonation":
            return `Removed donation ${change.remove_donation_id?.slice(0, 8) ?? ""}`;
        default:
            return change.kind;
    }
}

function totalCash(ct: CashTotal): number {
    const counts = ct.counts as Counts;
    return Object.entries(counts).reduce((s, [name, count]) => {
        const d = DENOMINATIONS.find((x) => x.name === name);
        return s + (d?.pence ?? 0) * (count ?? 0);
    }, 0);
}